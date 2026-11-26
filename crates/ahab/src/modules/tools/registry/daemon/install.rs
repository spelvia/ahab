//! Installing tools, as mason.nvim does: per machine, into
//! `<data>/tools/<id>/<version>/` with a `current` link, the version the
//! record pins, downloads checked against its SHA-256. Never system-wide,
//! never with `sudo`, never a runtime.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::Command;

use crate::modules::tools::registry::api::ToolJobDto;
use crate::modules::tools::registry::daemon::environment::{Vars, which};
use crate::modules::tools::registry::daemon::find::{INSTALLED_FILE, InstalledTool};
use crate::modules::tools::registry::model::{Asset, Source, ToolRecord, platform};
use crate::shared::sync::lock;

/// The output kept per job.
const OUTPUT_LINES: usize = 60;
/// The largest file a download may be.
const MAX_DOWNLOAD: u64 = 1 << 30;
/// How long a package manager may take.
const INSTALL_TIMEOUT: Duration = Duration::from_mins(30);

/// The jobs of this daemon, by tool.
pub struct Installs {
    tools_dir: PathBuf,
    jobs: Mutex<BTreeMap<String, ToolJobDto>>,
}

/// Why an install stopped: the step and what went wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub step: String,
    pub message: String,
}

fn fail(step: &str, message: impl Into<String>) -> Failure {
    Failure {
        step: step.to_owned(),
        message: message.into(),
    }
}

/// What installing needs from the daemon.
pub struct Installer<'a> {
    pub record: &'a ToolRecord,
    /// The machine's environment: where npm, pip, cargo and go are.
    pub vars: &'a Vars,
    /// Node for npm when the machine's `PATH` has none (`[plugins] node`).
    pub plugin_node: Option<&'a Path>,
    /// Where GitHub releases are downloaded from (tests serve their own).
    pub github: &'a str,
    /// Tells the job's progress.
    pub progress: &'a (dyn Fn(&ToolJobDto) + Send + Sync),
}

impl Installs {
    pub fn new(tools_dir: PathBuf) -> Self {
        Self {
            tools_dir,
            jobs: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn jobs(&self) -> Vec<ToolJobDto> {
        lock(&self.jobs).values().cloned().collect()
    }

    /// Starts a job for `tool`; false when one runs already.
    pub fn begin(&self, tool: &str, action: &str) -> bool {
        let mut jobs = lock(&self.jobs);
        if jobs.get(tool).is_some_and(|job| job.state == "running") {
            return false;
        }
        jobs.insert(
            tool.to_owned(),
            ToolJobDto {
                tool: tool.to_owned(),
                action: action.to_owned(),
                step: "starting".to_owned(),
                state: "running".to_owned(),
                message: None,
                output: Vec::new(),
            },
        );
        true
    }

    fn update(&self, tool: &str, change: impl FnOnce(&mut ToolJobDto)) -> Option<ToolJobDto> {
        let mut jobs = lock(&self.jobs);
        let job = jobs.get_mut(tool)?;
        change(job);
        Some(job.clone())
    }

    /// Runs `action` (`install`, `update`, `remove`) for the job begun.
    pub async fn run(&self, action: &str, installer: &Installer<'_>) -> Result<(), Failure> {
        let tool = &installer.record.id;
        let tell = |step: &str, line: Option<String>| {
            if let Some(job) = self.update(tool, |job| {
                step.clone_into(&mut job.step);
                if let Some(line) = line {
                    job.output.push(line);
                    let extra = job.output.len().saturating_sub(OUTPUT_LINES);
                    job.output.drain(..extra);
                }
            }) {
                (installer.progress)(&job);
            }
        };
        let result = match action {
            "remove" => {
                tell("removing", None);
                remove(&self.tools_dir, tool)
            }
            _ => self.install(installer, &tell).await,
        };
        let job = self.update(tool, |job| match &result {
            Ok(()) => {
                "done".clone_into(&mut job.state);
                "done".clone_into(&mut job.step);
            }
            Err(failure) => {
                "failed".clone_into(&mut job.state);
                job.step.clone_from(&failure.step);
                job.message = Some(failure.message.clone());
            }
        });
        if let Some(job) = job {
            (installer.progress)(&job);
        }
        result
    }

    async fn install(
        &self,
        installer: &Installer<'_>,
        tell: &(dyn Fn(&str, Option<String>) + Sync),
    ) -> Result<(), Failure> {
        let record = installer.record;
        let Some(source) = &record.source else {
            return Err(fail("check", "Your own tools are found, not installed."));
        };
        let Some(version) = source.version().map(str::to_owned) else {
            return Err(fail(
                "check",
                format!(
                    "Ahab does not install {}{}",
                    record.name,
                    record
                        .hint
                        .as_ref()
                        .map(|hint| format!(": {hint}"))
                        .unwrap_or_default()
                ),
            ));
        };
        let platform = platform();
        if !source.installable_on(&platform) {
            return Err(fail(
                "check",
                format!("The registry has no checked build of it for {platform}."),
            ));
        }
        let folder = self.tools_dir.join(&record.id);
        let target = folder.join(&version);
        if !target.join(INSTALLED_FILE).is_file() {
            let partial = folder.join(format!(".{version}.partial"));
            let _ = std::fs::remove_dir_all(&partial);
            std::fs::create_dir_all(&partial)
                .map_err(|error| fail("prepare", format!("cannot create its folder: {error}")))?;
            let bin = match self
                .fetch(source, &version, &partial, installer, tell)
                .await
            {
                Ok(bin) => bin,
                Err(failure) => {
                    let _ = std::fs::remove_dir_all(&partial);
                    return Err(failure);
                }
            };
            for relative in bin.values() {
                let path = partial.join(relative);
                if !path.is_file() {
                    let _ = std::fs::remove_dir_all(&partial);
                    return Err(fail("check", format!("the install has no {relative}")));
                }
                make_executable(&path);
            }
            let described = InstalledTool {
                id: record.id.clone(),
                version: version.clone(),
                bin,
                source: source.type_name().to_owned(),
                installed_at: time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .ok(),
            };
            let text = serde_json::to_string_pretty(&described)
                .map_err(|error| fail("finish", error.to_string()))?;
            std::fs::write(partial.join(INSTALLED_FILE), text)
                .map_err(|error| fail("finish", error.to_string()))?;
            let _ = std::fs::remove_dir_all(&target);
            std::fs::rename(&partial, &target)
                .map_err(|error| fail("finish", format!("cannot move it in place: {error}")))?;
        }
        tell("linking", None);
        point_current(&folder, &version).map_err(|error| fail("finish", error))?;
        // The older versions go; running servers restart on the new one.
        if let Ok(entries) = std::fs::read_dir(&folder) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name != version && name != "current" && entry.path().is_dir() {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
        Ok(())
    }

    /// Puts the tool's files in `into`; the binaries' paths in it.
    async fn fetch(
        &self,
        source: &Source,
        version: &str,
        into: &Path,
        installer: &Installer<'_>,
        tell: &(dyn Fn(&str, Option<String>) + Sync),
    ) -> Result<BTreeMap<String, String>, Failure> {
        let record = installer.record;
        let bins = |overrides: &BTreeMap<String, String>| -> BTreeMap<String, String> {
            let base = if overrides.is_empty() {
                &record.bin
            } else {
                overrides
            };
            let mut bins: BTreeMap<String, String> = base
                .iter()
                .map(|(name, path)| (name.clone(), path.replace("${version}", version)))
                .collect();
            if bins.is_empty()
                && let Some(program) = record.program()
            {
                bins.insert(program.to_owned(), program.to_owned());
            }
            bins
        };
        let path = installer.vars.get("PATH").cloned().unwrap_or_default();
        let tool = |name: &str, runtime: &str| {
            which(name, &path).ok_or_else(|| {
                fail(
                    "check",
                    format!(
                        "Installing it needs {runtime} ({name} is not on this machine's PATH)."
                    ),
                )
            })
        };
        match source {
            Source::Npm { package, extra, .. } => {
                let npm = which("npm", &path)
                    .or_else(|| {
                        installer
                            .plugin_node
                            .and_then(Path::parent)
                            .map(|dir| dir.join("npm"))
                            .filter(|npm| npm.is_file())
                    })
                    .ok_or_else(|| fail("check", "Installing it needs Node (npm)."))?;
                let mut args = vec![
                    "install".to_owned(),
                    "--prefix".to_owned(),
                    into.display().to_string(),
                    "--no-audit".to_owned(),
                    "--no-fund".to_owned(),
                    "--no-save".to_owned(),
                    format!("{package}@{version}"),
                ];
                args.extend(extra.iter().cloned());
                tell("installing with npm", None);
                self.command(&npm, &args, into, installer.vars, None, tell)
                    .await?;
                Ok(bins(&BTreeMap::new()))
            }
            Source::Pypi { package, .. } => {
                let python = tool("python3", "Python 3")?;
                let venv = into.join("venv");
                tell("creating a virtual environment", None);
                self.command(
                    &python,
                    &[
                        "-m".to_owned(),
                        "venv".to_owned(),
                        venv.display().to_string(),
                    ],
                    into,
                    installer.vars,
                    None,
                    tell,
                )
                .await?;
                tell("installing with pip", None);
                self.command(
                    &venv.join("bin/pip"),
                    &[
                        "install".to_owned(),
                        "--disable-pip-version-check".to_owned(),
                        format!("{package}=={version}"),
                    ],
                    into,
                    installer.vars,
                    None,
                    tell,
                )
                .await?;
                Ok(bins(&BTreeMap::new()))
            }
            Source::Cargo {
                krate,
                git,
                features,
                ..
            } => {
                let cargo = tool("cargo", "Rust (cargo)")?;
                tell("building with cargo", None);
                self.command(
                    &cargo,
                    &cargo_args(krate, version, git.as_deref(), features, into),
                    into,
                    installer.vars,
                    None,
                    tell,
                )
                .await?;
                Ok(bins(&BTreeMap::new()))
            }
            Source::Go { module, .. } => {
                let go = tool("go", "Go")?;
                tell("building with go", None);
                self.command(
                    &go,
                    &["install".to_owned(), format!("{module}@{version}")],
                    into,
                    installer.vars,
                    Some(("GOBIN", into)),
                    tell,
                )
                .await?;
                Ok(bins(&BTreeMap::new()))
            }
            Source::GithubRelease { repo, tag, .. } => {
                let asset = source
                    .asset(&platform())
                    .ok_or_else(|| fail("check", "no build for this platform"))?;
                let file = asset
                    .file
                    .as_deref()
                    .ok_or_else(|| fail("check", "the record names no file"))?
                    .replace("${version}", version);
                let tag = tag
                    .as_deref()
                    .unwrap_or("${version}")
                    .replace("${version}", version);
                let url = format!(
                    "{}/{repo}/releases/download/{}/{file}",
                    installer.github.trim_end_matches('/'),
                    encode_tag(&tag)
                );
                let bin = bins(&asset.bin);
                self.download_and_unpack(&url, &file, asset, into, &bin, tell)
                    .await?;
                Ok(bin)
            }
            Source::Download { .. } => {
                let asset = source
                    .asset(&platform())
                    .ok_or_else(|| fail("check", "no build for this platform"))?;
                let url = asset
                    .url
                    .as_deref()
                    .ok_or_else(|| fail("check", "the record names no address"))?
                    .replace("${version}", version);
                let file = url.rsplit('/').next().unwrap_or("download").to_owned();
                let bin = bins(&asset.bin);
                self.download_and_unpack(&url, &file, asset, into, &bin, tell)
                    .await?;
                Ok(bin)
            }
            Source::System | Source::Unknown => {
                Err(fail("check", "Ahab does not install this kind of tool."))
            }
        }
    }

    async fn download_and_unpack(
        &self,
        url: &str,
        file: &str,
        asset: &Asset,
        into: &Path,
        bin: &BTreeMap<String, String>,
        tell: &(dyn Fn(&str, Option<String>) + Sync),
    ) -> Result<(), Failure> {
        let Some(expected) = asset.sha256.as_deref() else {
            return Err(fail("check", "the record has no checksum for this file"));
        };
        tell("downloading", Some(url.to_owned()));
        let download = into.join(".download");
        let actual = download_to(url, &download).await?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&download);
            return Err(fail(
                "verifying",
                format!("the file's SHA-256 is {actual}, not the record's {expected}"),
            ));
        }
        tell("unpacking", None);
        let first_bin = bin.values().next().cloned();
        let into = into.to_path_buf();
        let file = file.to_owned();
        tokio::task::spawn_blocking(move || {
            let result = unpack(&download, &file, &into, first_bin.as_deref());
            let _ = std::fs::remove_file(&download);
            result
        })
        .await
        .map_err(|error| fail("unpacking", error.to_string()))?
        .map_err(|error| fail("unpacking", error))
    }

    async fn command(
        &self,
        program: &Path,
        args: &[String],
        cwd: &Path,
        vars: &Vars,
        extra: Option<(&str, &Path)>,
        tell: &(dyn Fn(&str, Option<String>) + Sync),
    ) -> Result<(), Failure> {
        let step = program
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .envs(vars)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some((name, value)) = extra {
            command.env(name, value);
        }
        let mut child = command
            .spawn()
            .map_err(|error| fail(&step, format!("cannot run {step}: {error}")))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let lines = Mutex::new(Vec::<String>::new());
        let running = format!("running {step}");
        let status = tokio::time::timeout(INSTALL_TIMEOUT, async {
            tokio::join!(
                pump(stdout, &running, tell, &lines),
                pump(stderr, &running, tell, &lines),
            );
            child.wait().await
        })
        .await
        .map_err(|_| fail(&step, format!("{step} took too long")))?
        .map_err(|error| fail(&step, error.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            let last = lock(&lines)
                .iter()
                .rev()
                .find(|line| !line.trim().is_empty())
                .cloned();
            Err(fail(
                &step,
                format!(
                    "{step} failed ({status}){}",
                    last.map(|line| format!(": {line}")).unwrap_or_default()
                ),
            ))
        }
    }
}

/// Reads a command's output line by line, telling each.
async fn pump<R: tokio::io::AsyncRead + Unpin>(
    stream: Option<R>,
    step: &str,
    tell: &(dyn Fn(&str, Option<String>) + Sync),
    lines: &Mutex<Vec<String>>,
) {
    let Some(stream) = stream else { return };
    let mut reader = BufReader::new(stream).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        tell(step, Some(line.clone()));
        lock(lines).push(line);
    }
}

/// `cargo install`'s arguments.
pub fn cargo_args(
    krate: &str,
    version: &str,
    git: Option<&str>,
    features: &[String],
    into: &Path,
) -> Vec<String> {
    let mut args = vec![
        "install".to_owned(),
        "--locked".to_owned(),
        "--root".to_owned(),
        into.display().to_string(),
    ];
    match git {
        Some(git) => args.extend([
            "--git".to_owned(),
            git.to_owned(),
            "--tag".to_owned(),
            version.to_owned(),
        ]),
        None => args.extend(["--version".to_owned(), version.to_owned()]),
    }
    if !features.is_empty() {
        args.extend(["--features".to_owned(), features.join(",")]);
    }
    args.push(krate.to_owned());
    args
}

/// A tag in a URL: `@biomejs/biome@2.6.8` keeps its `@` and `/` escaped.
fn encode_tag(tag: &str) -> String {
    tag.replace('/', "%2F")
}

/// Downloads `url` to `to`, its SHA-256 as hex.
async fn download_to(url: &str, to: &Path) -> Result<String, Failure> {
    use sha2::Digest as _;
    let client = reqwest::Client::builder()
        .user_agent(concat!("ahab/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| fail("downloading", error.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| fail("downloading", format!("cannot download {url}: {error}")))?;
    if !response.status().is_success() {
        return Err(fail(
            "downloading",
            format!("{url} answered {}", response.status()),
        ));
    }
    let mut file =
        std::fs::File::create(to).map_err(|error| fail("downloading", error.to_string()))?;
    let mut hash = sha2::Sha256::new();
    let mut size = 0u64;
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| fail("downloading", error.to_string()))?
    {
        size += chunk.len() as u64;
        if size > MAX_DOWNLOAD {
            return Err(fail("downloading", "the file is too large"));
        }
        hash.update(&chunk);
        file.write_all(&chunk)
            .map_err(|error| fail("downloading", error.to_string()))?;
    }
    Ok(super::hex(&hash.finalize()))
}

/// Unpacks a downloaded `file` (named as released) into `into`: archives
/// as they are, a single (possibly gzipped) file as the first binary.
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the name is lower-cased first, and `.tar.gz` is two extensions"
)]
pub fn unpack(
    download: &Path,
    file: &str,
    into: &Path,
    first_bin: Option<&str>,
) -> Result<(), String> {
    let open = || std::fs::File::open(download).map_err(|error| error.to_string());
    let file = file.to_ascii_lowercase();
    if file.ends_with(".tar.gz") || file.ends_with(".tgz") {
        let decoder = flate2::read::GzDecoder::new(open()?);
        safe_untar(tar::Archive::new(decoder), into)
    } else if file.ends_with(".tar") {
        safe_untar(tar::Archive::new(open()?), into)
    } else if file.ends_with(".zip") {
        let mut archive = zip::ZipArchive::new(open()?).map_err(|error| error.to_string())?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            let Some(name) = entry.enclosed_name() else {
                return Err(format!(
                    "the archive has an unsafe path: {}",
                    entry
                        .name()
                        .map(std::borrow::Cow::into_owned)
                        .unwrap_or_default()
                ));
            };
            let target = into.join(name);
            if entry.is_dir() {
                std::fs::create_dir_all(&target).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut out = std::fs::File::create(&target).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|error| error.to_string())?;
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(mode));
            }
        }
        Ok(())
    } else {
        let name = first_bin.ok_or("the record names no binary")?;
        let target = into.join(name);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut out = std::fs::File::create(&target).map_err(|error| error.to_string())?;
        if file.ends_with(".gz") {
            let mut decoder = flate2::read::GzDecoder::new(open()?);
            std::io::copy(&mut decoder, &mut out).map_err(|error| error.to_string())?;
        } else {
            std::io::copy(&mut open()?, &mut out).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// Unpacks a tar archive, refusing entries that would land outside `into`.
fn safe_untar<R: std::io::Read>(mut archive: tar::Archive<R>, into: &Path) -> Result<(), String> {
    for entry in archive.entries().map_err(|error| error.to_string())? {
        let mut entry = entry.map_err(|error| error.to_string())?;
        let unpacked = entry.unpack_in(into).map_err(|error| error.to_string())?;
        if !unpacked {
            return Err("the archive has an unsafe path".to_owned());
        }
    }
    Ok(())
}

fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(metadata) = std::fs::metadata(path) {
            let mode = metadata.permissions().mode() | 0o755;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Points `<folder>/current` at `version`, replacing the link in one step.
fn point_current(folder: &Path, version: &str) -> Result<(), String> {
    let link = folder.join("current");
    let next = folder.join(".current.new");
    let _ = std::fs::remove_file(&next);
    #[cfg(unix)]
    std::os::unix::fs::symlink(version, &next).map_err(|error| error.to_string())?;
    #[cfg(not(unix))]
    std::fs::write(&next, version).map_err(|error| error.to_string())?;
    std::fs::rename(&next, &link).map_err(|error| error.to_string())
}

/// Deletes a tool's folder.
fn remove(tools_dir: &Path, id: &str) -> Result<(), Failure> {
    let folder = tools_dir.join(id);
    match std::fs::remove_dir_all(&folder) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(fail("removing", error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_installs_the_pinned_version_or_tag() {
        let into = Path::new("/data/tools/nil/.x");
        assert_eq!(
            cargo_args("taplo-cli", "0.10.0", None, &[], into).join(" "),
            "install --locked --root /data/tools/nil/.x --version 0.10.0 taplo-cli"
        );
        assert_eq!(
            cargo_args(
                "nil",
                "2026-07-23",
                Some("https://github.com/oxalica/nil"),
                &[],
                into
            )
            .join(" "),
            "install --locked --root /data/tools/nil/.x --git https://github.com/oxalica/nil --tag 2026-07-23 nil"
        );
    }

    #[test]
    fn single_files_become_the_binary_and_archives_stay_inside() {
        let dir = tempfile::tempdir().unwrap();
        let gz = dir.path().join("dl");
        let mut encoder = flate2::write::GzEncoder::new(
            std::fs::File::create(&gz).unwrap(),
            flate2::Compression::default(),
        );
        encoder.write_all(b"#!/bin/sh\necho hi\n").unwrap();
        encoder.finish().unwrap();
        let into = dir.path().join("out");
        std::fs::create_dir_all(&into).unwrap();
        unpack(&gz, "tool-x86_64.gz", &into, Some("tool")).unwrap();
        assert_eq!(
            std::fs::read_to_string(into.join("tool")).unwrap(),
            "#!/bin/sh\necho hi\n"
        );

        let tarball = dir.path().join("t.tar");
        {
            let mut builder = tar::Builder::new(std::fs::File::create(&tarball).unwrap());
            let mut header = tar::Header::new_gnu();
            let data = b"bin";
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "tool-1.0/bin/tool", &data[..])
                .unwrap();
            builder.finish().unwrap();
        }
        unpack(&tarball, "tool-1.0.tar", &into, None).unwrap();
        assert!(into.join("tool-1.0/bin/tool").is_file());
    }

    #[test]
    fn current_points_at_the_installed_version() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("1.0")).unwrap();
        std::fs::create_dir_all(dir.path().join("2.0")).unwrap();
        point_current(dir.path(), "1.0").unwrap();
        point_current(dir.path(), "2.0").unwrap();
        assert_eq!(
            std::fs::canonicalize(dir.path().join("current")).unwrap(),
            std::fs::canonicalize(dir.path().join("2.0")).unwrap()
        );
    }
}
