//! Terminals on a machine: the terminal host, a small process of
//! Ahab's own (`ahab terminal-host`) that owns every terminal's PTY so they
//! outlive the daemon, and the daemon's client to it. They talk over a Unix
//! socket in the data folder, in frames: a kind, a length, a payload.
//!
//! - `REQUEST` (JSON) asks the host something; `REPLY` (JSON) answers.
//! - After an `attach` request, the host sends `EVENT` `reset`, the
//!   terminal's recent output as `OUTPUT`, then live `OUTPUT` and `EVENT`s;
//!   the client sends `INPUT` and `RESIZE`.
//! - After a `watch` request, the host sends every terminal's `EVENT`s.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::shared::error::{DaemonError, fail};
use crate::shared::sync::lock;

/// Frame kinds, on the socket and (without the length) on the channel to
/// the server.
pub mod kind {
    pub const REQUEST: u8 = 0;
    pub const REPLY: u8 = 1;
    pub const OUTPUT: u8 = 2;
    pub const INPUT: u8 = 3;
    pub const RESIZE: u8 = 4;
    pub const EVENT: u8 = 5;
}

/// The host's protocol version; a daemon checks it on `hello`.
pub const PROTOCOL: u32 = 1;

/// Recent rows kept as cells for the terminal emulator.
const SCROLLBACK: usize = 2_048;
/// Older, formatted rows are kept in two rotating temporary files.
const HISTORY_SEGMENT: usize = 2 * 1024 * 1024;
const HISTORY_ROWS: usize = (10_000 - SCROLLBACK) / 2;

/// How long the host stays without any terminal before it exits.
const IDLE: Duration = Duration::from_mins(5);

/// The largest frame either side accepts.
const MAX_FRAME: usize = 16 * 1024 * 1024;

/// The socket's file name in the data folder.
pub const SOCKET: &str = "terminal-host.sock";

/// Writes one frame.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    kind: u8,
    payload: &[u8],
) -> std::io::Result<()> {
    let length = u32::try_from(payload.len()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(kind);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(payload);
    writer.write_all(&frame).await
}

/// Reads one frame; `None` at the end of the stream.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> std::io::Result<Option<(u8, Vec<u8>)>> {
    let mut head = [0u8; 5];
    match reader.read_exact(&mut head).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let length = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if length > MAX_FRAME {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).await?;
    Ok(Some((head[0], payload)))
}

/// What a terminal runs, and where.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spec {
    pub cwd: PathBuf,
    /// A command for the shell to run; the login shell itself otherwise.
    pub command: Option<String>,
    /// Variables added to the machine user's environment.
    pub env: HashMap<String, String>,
}

/// A terminal as the host lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub id: Uuid,
    /// Its exit code, once its process ended.
    pub exited: Option<i32>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Request {
    Hello,
    Create {
        id: Uuid,
        spec: Spec,
        cols: u16,
        rows: u16,
    },
    List,
    Kill {
        id: Uuid,
    },
    Restart {
        id: Uuid,
    },
    Remove {
        id: Uuid,
    },
    Attach {
        id: Uuid,
    },
    Watch,
}

// ---------------------------------------------------------------------------
// The host.

#[derive(Default)]
struct DiskHistory {
    previous: Option<File>,
    current: Option<File>,
    bytes: usize,
    rows: usize,
    unavailable: bool,
}

impl DiskHistory {
    fn append(&mut self, row: &[u8]) -> std::io::Result<()> {
        if self.unavailable || row.len() > HISTORY_SEGMENT {
            return Ok(());
        }
        if self.current.is_none()
            || self.bytes + row.len() > HISTORY_SEGMENT
            || self.rows >= HISTORY_ROWS
        {
            self.previous = self.current.replace(tempfile::tempfile()?);
            self.bytes = 0;
            self.rows = 0;
        }
        if let Some(file) = self.current.as_mut() {
            file.write_all(row)?;
            self.bytes += row.len();
            self.rows += 1;
        }
        Ok(())
    }

    fn replay(&mut self, output: &mut Vec<u8>) -> std::io::Result<()> {
        for file in [&mut self.previous, &mut self.current]
            .into_iter()
            .flatten()
        {
            file.rewind()?;
            file.read_to_end(output)?;
        }
        Ok(())
    }

    fn disable(&mut self, error: &std::io::Error) {
        tracing::warn!(%error, "cannot use the terminal's disk history");
        *self = Self {
            unavailable: true,
            ..Self::default()
        };
    }
}

#[derive(Default)]
struct Titles {
    title: Option<String>,
    history: DiskHistory,
    row: String,
}

impl vt100::Callbacks for Titles {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        self.title = Some(String::from_utf8_lossy(title).trim().to_owned());
    }

    fn on_scroll(&mut self, contents: vt100::capture::RowContents<'_>, alternate_screen: bool) {
        if alternate_screen || self.history.unavailable {
            return;
        }
        self.row.clear();
        let _ = contents.write_formatted_basic(
            &mut self.row,
            &mut vt100::capture::BasicFormattedCaptureState::default(),
        );
        self.row.push_str("\x1b[m\r\n");
        if let Err(error) = self.history.append(self.row.as_bytes()) {
            self.history.disable(&error);
        }
    }
}

/// ED 3 and RIS also clear the archived rows. Keeping a parser across
/// reads handles escape sequences split between PTY chunks.
#[derive(Default, PartialEq, Eq)]
enum ClearHistory {
    #[default]
    None,
    Scrollback,
    Terminal,
}

impl vte::Perform for ClearHistory {
    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        if !ignore
            && action == 'J'
            && (intermediates.is_empty() || intermediates == b"?")
            && params.iter().next().and_then(|first| first.first()) == Some(&3)
        {
            *self = Self::Scrollback;
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if !ignore && intermediates.is_empty() && byte == b'c' {
            *self = Self::Terminal;
        }
    }

    fn terminated(&self) -> bool {
        *self != Self::None
    }
}

/// A terminal's screen and history as a terminal would show them (`vt100`),
/// so a new attachment gets them at the terminal's size of today, not as
/// they were drawn at other sizes.
struct Screen {
    parser: vt100::Parser<Titles>,
    clear_detector: vte::Parser,
    snapshot: Option<Weak<Vec<u8>>>,
}

fn nonzero(value: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(value).unwrap_or(std::num::NonZeroU16::MIN)
}

impl Screen {
    fn new(cols: u16, rows: u16) -> Self {
        Self {
            parser: vt100::Parser::new_with_callbacks(
                nonzero(rows),
                nonzero(cols),
                SCROLLBACK,
                Titles::default(),
            ),
            clear_detector: vte::Parser::new(),
            snapshot: None,
        }
    }

    fn process(&mut self, mut bytes: &[u8]) {
        self.snapshot = None;
        while !bytes.is_empty() {
            let mut clear = ClearHistory::default();
            let consumed = self
                .clear_detector
                .advance_until_terminated(&mut clear, bytes);
            if consumed == 0 {
                self.parser.process(bytes);
                break;
            }
            self.parser.process(&bytes[..consumed]);
            if clear == ClearHistory::Terminal
                || (clear == ClearHistory::Scrollback && !self.parser.screen().alternate_screen())
            {
                self.parser.callbacks_mut().history = DiskHistory::default();
            }
            bytes = &bytes[consumed..];
        }
    }

    fn resize(&mut self, cols: u16, rows: u16) {
        self.snapshot = None;
        self.parser
            .screen_mut()
            .set_size(nonzero(rows), nonzero(cols));
    }

    /// Escape codes that draw the history (into a terminal's scrollback),
    /// then the screen as it is, cursor and modes included.
    fn snapshot(&mut self) -> Arc<Vec<u8>> {
        if let Some(held) = self.snapshot.as_ref().and_then(Weak::upgrade) {
            return held;
        }
        let mut archive = Vec::new();
        if !self.parser.screen().alternate_screen() {
            let disk = &mut self.parser.callbacks_mut().history;
            if let Err(error) = disk.replay(&mut archive) {
                archive.clear();
                disk.disable(&error);
            }
        }
        let had_archive = !archive.is_empty();
        let screen = self.parser.screen_mut();
        let (rows, cols) = screen.size();
        let (rows, cols) = (usize::from(rows.get()), cols.get());
        let mut out = String::new();
        if !screen.alternate_screen() {
            screen.set_scrollback(usize::MAX);
            let history = screen.scrollback();
            let mut offset = history;
            while offset > 0 {
                screen.set_scrollback(offset);
                let take = offset.min(rows);
                for row in screen.rows_formatted(0, cols).take(take) {
                    out.push_str(&row);
                    out.push_str("\x1b[m\r\n");
                }
                offset -= take;
            }
            screen.set_scrollback(0);
            if history > 0 || had_archive {
                // The history scrolls off the screen, into the scrollback.
                out.push_str(&"\r\n".repeat(rows.saturating_sub(1)));
            }
        }
        out.push_str(&screen.state_formatted());
        archive.extend_from_slice(out.as_bytes());
        let snapshot = Arc::new(archive);
        self.snapshot = Some(Arc::downgrade(&snapshot));
        snapshot
    }
}

/// What a terminal's attachments receive.
#[derive(Clone, Debug)]
enum Out {
    Bytes(Arc<[u8]>),
    Event(Value),
}

/// One running (or exited) process and its PTY.
struct Process {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

struct Terminal {
    id: Uuid,
    spec: Spec,
    size: Mutex<(u16, u16)>,
    process: Mutex<Option<Process>>,
    exit: Mutex<Option<i32>>,
    screen: Mutex<Screen>,
    /// The title last told, so only changes are.
    title: Mutex<Option<String>>,
    output: broadcast::Sender<Out>,
    /// Bumped on each start, so an old process's end is not the new one's.
    generation: Mutex<u64>,
}

impl Terminal {
    /// The shell set a title: told to attachments and the daemon, if new.
    fn retitled(&self, host: &Host, title: String) {
        let mut last = lock(&self.title);
        if last.as_deref() == Some(title.as_str()) || title.is_empty() {
            return;
        }
        *last = Some(title.clone());
        let event = json!({ "type": "title", "terminal": self.id, "title": title });
        let _ = self.output.send(Out::Event(event.clone()));
        let _ = host.events.send(event);
    }
}

struct Host {
    terminals: Mutex<HashMap<Uuid, Arc<Terminal>>>,
    events: broadcast::Sender<Value>,
    empty_since: Mutex<Option<Instant>>,
}

/// The shell to start: the user's login shell, or `/bin/sh`.
fn shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_owned())
}

impl Host {
    /// Starts the terminal's process (again).
    fn start(self: &Arc<Self>, terminal: &Arc<Terminal>) -> Result<(), String> {
        let (cols, rows) = *lock(&terminal.size);
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("cannot open a terminal: {error}"))?;
        let shell = shell();
        let mut command = CommandBuilder::new(&shell);
        match &terminal.spec.command {
            Some(line) => command.args(["-lc", line]),
            None => command.arg("-l"),
        }
        command.cwd(&terminal.spec.cwd);
        // Ahab's own settings stay with Ahab.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("AHAB_") {
                command.env_remove(key);
            }
        }
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        for (key, value) in &terminal.spec.env {
            command.env(key, value);
        }
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("cannot start {shell}: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;
        let killer = child.clone_killer();
        let generation = {
            let mut generation = lock(&terminal.generation);
            *generation += 1;
            *generation
        };
        *lock(&terminal.exit) = None;
        *lock(&terminal.process) = Some(Process {
            master: pair.master,
            writer,
            killer,
        });

        // Output: kept for new attachments, sent to those attached.
        let reading = Arc::clone(terminal);
        let title_host = Arc::clone(self);
        std::thread::spawn(move || {
            let mut buffer = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let bytes: Arc<[u8]> = Arc::from(&buffer[..read]);
                        let title = {
                            let mut screen = lock(&reading.screen);
                            screen.process(&bytes);
                            let _ = reading.output.send(Out::Bytes(bytes));
                            screen.parser.callbacks_mut().title.take()
                        };
                        if let Some(title) = title {
                            reading.retitled(&title_host, title);
                        }
                    }
                }
            }
        });

        // Its end: recorded, and told to attachments and the daemon.
        let host = Arc::clone(self);
        let waiting = Arc::clone(terminal);
        std::thread::spawn(move || {
            let code = child.wait().map_or(-1, |status| {
                i32::try_from(status.exit_code()).unwrap_or(i32::MAX)
            });
            if *lock(&waiting.generation) != generation {
                return;
            }
            *lock(&waiting.exit) = Some(code);
            let event = json!({ "type": "exited", "terminal": waiting.id, "code": code });
            let _ = waiting.output.send(Out::Event(event.clone()));
            let _ = host.events.send(event);
        });
        Ok(())
    }

    fn get(&self, id: Uuid) -> Result<Arc<Terminal>, String> {
        lock(&self.terminals)
            .get(&id)
            .cloned()
            .ok_or_else(|| "There is no such terminal on this machine.".to_owned())
    }

    fn kill(terminal: &Terminal) {
        if let Some(process) = lock(&terminal.process).as_mut() {
            let _ = process.killer.kill();
        }
    }

    fn noted_emptiness(&self) {
        let empty = lock(&self.terminals).is_empty();
        let mut since = lock(&self.empty_since);
        *since = if empty {
            Some(since.unwrap_or_else(Instant::now))
        } else {
            None
        };
    }

    /// Answers one request; `attach` and `watch` are answered by the
    /// connection itself.
    fn answer(self: &Arc<Self>, request: Request) -> Result<Value, String> {
        match request {
            Request::Hello => Ok(json!({ "protocol": PROTOCOL })),
            Request::Create {
                id,
                spec,
                cols,
                rows,
            } => {
                let (output, _) = broadcast::channel(1024);
                let terminal = Arc::new(Terminal {
                    id,
                    spec,
                    size: Mutex::new((cols.max(2), rows.max(1))),
                    process: Mutex::new(None),
                    exit: Mutex::new(None),
                    screen: Mutex::new(Screen::new(cols.max(2), rows.max(1))),
                    title: Mutex::new(None),
                    output,
                    generation: Mutex::new(0),
                });
                self.start(&terminal)?;
                lock(&self.terminals).insert(id, terminal);
                self.noted_emptiness();
                Ok(json!({}))
            }
            Request::List => {
                let list: Vec<Listed> = lock(&self.terminals)
                    .values()
                    .map(|terminal| {
                        let (cols, rows) = *lock(&terminal.size);
                        Listed {
                            id: terminal.id,
                            exited: *lock(&terminal.exit),
                            cols,
                            rows,
                        }
                    })
                    .collect();
                Ok(json!({ "terminals": list }))
            }
            Request::Kill { id } => {
                Self::kill(&*self.get(id)?);
                Ok(json!({}))
            }
            Request::Restart { id } => {
                let terminal = self.get(id)?;
                Self::kill(&terminal);
                let line = b"\r\n\x1b[2m[restarted]\x1b[0m\r\n";
                {
                    let mut screen = lock(&terminal.screen);
                    screen.process(line);
                    let _ = terminal.output.send(Out::Bytes(Arc::from(&line[..])));
                }
                self.start(&terminal)?;
                Ok(json!({}))
            }
            Request::Remove { id } => {
                if let Some(terminal) = lock(&self.terminals).remove(&id) {
                    Self::kill(&terminal);
                }
                self.noted_emptiness();
                Ok(json!({}))
            }
            Request::Attach { .. } | Request::Watch => Ok(json!({})),
        }
    }
}

/// Sends an attachment what the terminal shows: a reset, then its recent
/// output, then whether it ended.
async fn snapshot<W: AsyncWrite + Unpin>(
    stream: &mut W,
    terminal: &Terminal,
) -> std::io::Result<broadcast::Receiver<Out>> {
    // Output is published under the screen lock. Subscribe at the same
    // instant as the snapshot so those bytes cannot be shown twice.
    let (bytes, output) = {
        let mut screen = lock(&terminal.screen);
        (screen.snapshot(), terminal.output.subscribe())
    };
    let size = *lock(&terminal.size);
    write_frame(stream, kind::EVENT, br#"{"type":"reset"}"#).await?;
    let size = json!({ "type": "size", "cols": size.0, "rows": size.1 }).to_string();
    write_frame(stream, kind::EVENT, size.as_bytes()).await?;
    for chunk in bytes.chunks(256 * 1024) {
        write_frame(stream, kind::OUTPUT, chunk).await?;
    }
    let exit = *lock(&terminal.exit);
    if let Some(code) = exit {
        let event = json!({ "type": "exited", "terminal": terminal.id, "code": code });
        write_frame(stream, kind::EVENT, event.to_string().as_bytes()).await?;
    }
    Ok(output)
}

/// One attachment: output out, input and resizes in, until either side ends.
async fn attached(mut stream: UnixStream, terminal: Arc<Terminal>) -> std::io::Result<()> {
    let mut output = snapshot(&mut stream, &terminal).await?;
    let (mut reader, mut writer) = stream.into_split();
    let input = Arc::clone(&terminal);
    let mut reading = tokio::spawn(async move {
        while let Ok(Some((frame, payload))) = read_frame(&mut reader).await {
            match frame {
                kind::INPUT => {
                    if let Some(process) = lock(&input.process).as_mut() {
                        let _ = process.writer.write_all(&payload);
                        let _ = process.writer.flush();
                    }
                }
                kind::RESIZE => {
                    let Ok(size) = serde_json::from_slice::<Value>(&payload) else {
                        continue;
                    };
                    let cols = size["cols"].as_u64().and_then(|n| u16::try_from(n).ok());
                    let rows = size["rows"].as_u64().and_then(|n| u16::try_from(n).ok());
                    if let (Some(cols), Some(rows)) = (cols, rows) {
                        let (cols, rows) = (cols.max(2), rows.max(1));
                        *lock(&input.size) = (cols, rows);
                        lock(&input.screen).resize(cols, rows);
                        if let Some(process) = lock(&input.process).as_ref() {
                            let _ = process.master.resize(PtySize {
                                rows,
                                cols,
                                pixel_width: 0,
                                pixel_height: 0,
                            });
                        }
                        // Every attachment learns the terminal's size.
                        let event = json!({ "type": "size", "cols": cols, "rows": rows });
                        let _ = input.output.send(Out::Event(event));
                    }
                }
                _ => {}
            }
        }
    });
    let result = loop {
        let event = tokio::select! {
            _ = &mut reading => break Ok(()),
            event = output.recv() => event,
        };
        match event {
            Ok(Out::Bytes(bytes)) => {
                if let Err(error) = write_frame(&mut writer, kind::OUTPUT, &bytes).await {
                    break Err(error);
                }
            }
            Ok(Out::Event(event)) => {
                let text = event.to_string();
                if let Err(error) = write_frame(&mut writer, kind::EVENT, text.as_bytes()).await {
                    break Err(error);
                }
            }
            // Too slow to keep up: it starts again from what the terminal shows.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                match snapshot(&mut writer, &terminal).await {
                    Ok(new_output) => output = new_output,
                    Err(error) => break Err(error),
                }
            }
            Err(broadcast::error::RecvError::Closed) => break Ok(()),
        }
    };
    reading.abort();
    result
}

/// Serves terminals on `socket` until a while after the last one is gone.
pub async fn serve_host(socket: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
    }
    let (events, _) = broadcast::channel(256);
    let host = Arc::new(Host {
        terminals: Mutex::default(),
        events,
        empty_since: Mutex::new(Some(Instant::now())),
    });
    let mut idle = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let Ok((stream, _)) = accepted else { continue };
                let host = Arc::clone(&host);
                tokio::spawn(async move {
                    if let Err(error) = connection(host, stream).await {
                        tracing::debug!(%error, "a terminal host connection ended");
                    }
                });
            }
            _ = idle.tick() => {
                let since = *lock(&host.empty_since);
                // Idle long enough, or its socket is gone (its data folder
                // was removed): no one can reach it any more.
                if since.is_some_and(|since| since.elapsed() > IDLE) || !socket.exists() {
                    let _ = std::fs::remove_file(socket);
                    return Ok(());
                }
            }
        }
    }
}

async fn connection(host: Arc<Host>, mut stream: UnixStream) -> std::io::Result<()> {
    let Some((kind::REQUEST, payload)) = read_frame(&mut stream).await? else {
        return Ok(());
    };
    let request: Request = match serde_json::from_slice(&payload) {
        Ok(request) => request,
        Err(error) => {
            let reply = json!({ "error": format!("unreadable request: {error}") });
            return write_frame(&mut stream, kind::REPLY, reply.to_string().as_bytes()).await;
        }
    };
    match request {
        Request::Attach { id } => {
            let terminal = match host.get(id) {
                Ok(terminal) => terminal,
                Err(error) => {
                    let reply = json!({ "error": error });
                    return write_frame(&mut stream, kind::REPLY, reply.to_string().as_bytes())
                        .await;
                }
            };
            write_frame(&mut stream, kind::REPLY, b"{}").await?;
            attached(stream, terminal).await
        }
        Request::Watch => {
            write_frame(&mut stream, kind::REPLY, b"{}").await?;
            let mut events = host.events.subscribe();
            // What happened before the watcher came is told first: titles
            // and ends (telling twice is harmless).
            let ended: Vec<Value> = lock(&host.terminals)
                .values()
                .flat_map(|terminal| {
                    let title = lock(&terminal.title).clone().map(
                        |title| json!({ "type": "title", "terminal": terminal.id, "title": title }),
                    );
                    let exit = lock(&terminal.exit).map(
                        |code| json!({ "type": "exited", "terminal": terminal.id, "code": code }),
                    );
                    title.into_iter().chain(exit)
                })
                .collect();
            for event in ended {
                write_frame(&mut stream, kind::EVENT, event.to_string().as_bytes()).await?;
            }
            loop {
                match events.recv().await {
                    Ok(event) => {
                        write_frame(&mut stream, kind::EVENT, event.to_string().as_bytes()).await?;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
        request => {
            let reply = match host.answer(request) {
                Ok(value) => value,
                Err(error) => json!({ "error": error }),
            };
            write_frame(&mut stream, kind::REPLY, reply.to_string().as_bytes()).await
        }
    }
}

// ---------------------------------------------------------------------------
// The daemon's side.

/// The daemon's way to its machine's terminal host, started when first
/// needed.
pub struct TerminalHost {
    socket: PathBuf,
    /// The `ahab` to start the host with; this executable by default.
    executable: Option<PathBuf>,
    starting: tokio::sync::Mutex<()>,
}

/// A stream attached to one terminal (see the module's description).
pub type Attached = UnixStream;

impl TerminalHost {
    pub fn new(data_dir: &Path, executable: Option<PathBuf>) -> Self {
        Self {
            socket: data_dir.join(SOCKET),
            executable,
            starting: tokio::sync::Mutex::new(()),
        }
    }

    /// A connection to the host, starting it when none runs.
    async fn connect(&self) -> Result<UnixStream, DaemonError> {
        if let Ok(stream) = UnixStream::connect(&self.socket).await {
            return Ok(stream);
        }
        let _starting = self.starting.lock().await;
        if let Ok(stream) = UnixStream::connect(&self.socket).await {
            return Ok(stream);
        }
        start_host(&self.socket, self.executable.as_deref())?;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(stream) = UnixStream::connect(&self.socket).await {
                return Ok(stream);
            }
        }
        Err(fail(
            "terminal_host_unavailable",
            "The terminal host did not start on this machine.",
        ))
    }

    /// Sends a request on a new connection; the reply and the connection.
    async fn ask(&self, request: Value) -> Result<(Value, UnixStream), DaemonError> {
        let mut stream = self.connect().await?;
        let broken = |error: std::io::Error| {
            fail(
                "terminal_host_unavailable",
                format!("The terminal host stopped answering: {error}"),
            )
        };
        write_frame(&mut stream, kind::REQUEST, request.to_string().as_bytes())
            .await
            .map_err(broken)?;
        let Some((kind::REPLY, payload)) = read_frame(&mut stream).await.map_err(broken)? else {
            return Err(fail(
                "terminal_host_unavailable",
                "The terminal host did not answer.",
            ));
        };
        let reply: Value = serde_json::from_slice(&payload).unwrap_or(Value::Null);
        if let Some(error) = reply.get("error").and_then(Value::as_str) {
            return Err(fail("terminal_failed", error));
        }
        Ok((reply, stream))
    }

    pub async fn create(
        &self,
        id: Uuid,
        spec: Spec,
        cols: u16,
        rows: u16,
    ) -> Result<(), DaemonError> {
        self.ask(json!({ "op": "create", "id": id, "spec": spec, "cols": cols, "rows": rows }))
            .await
            .map(|_| ())
    }

    /// The terminals the host has; none when it is not running.
    pub async fn list(&self) -> Result<Vec<Listed>, DaemonError> {
        if UnixStream::connect(&self.socket).await.is_err() {
            return Ok(Vec::new());
        }
        let (reply, _) = self.ask(json!({ "op": "list" })).await?;
        Ok(serde_json::from_value(reply["terminals"].clone()).unwrap_or_default())
    }

    pub async fn kill(&self, id: Uuid) -> Result<(), DaemonError> {
        self.ask(json!({ "op": "kill", "id": id }))
            .await
            .map(|_| ())
    }

    pub async fn restart(&self, id: Uuid) -> Result<(), DaemonError> {
        self.ask(json!({ "op": "restart", "id": id }))
            .await
            .map(|_| ())
    }

    pub async fn remove(&self, id: Uuid) -> Result<(), DaemonError> {
        if UnixStream::connect(&self.socket).await.is_err() {
            return Ok(());
        }
        self.ask(json!({ "op": "remove", "id": id }))
            .await
            .map(|_| ())
    }

    /// A stream attached to the terminal: a reset, its recent output, then
    /// live output and events.
    pub async fn attach(&self, id: Uuid) -> Result<Attached, DaemonError> {
        self.ask(json!({ "op": "attach", "id": id }))
            .await
            .map(|(_, stream)| stream)
    }

    /// Calls `emit` with each terminal's events (it ended), for as long as
    /// the daemon runs; it waits for the host while none runs.
    pub async fn watch(&self, emit: impl Fn(Value) + Send + Sync) {
        loop {
            if let Ok(mut stream) = UnixStream::connect(&self.socket).await
                && write_frame(&mut stream, kind::REQUEST, br#"{"op":"watch"}"#)
                    .await
                    .is_ok()
                && matches!(read_frame(&mut stream).await, Ok(Some((kind::REPLY, _))))
            {
                while let Ok(Some((kind::EVENT, payload))) = read_frame(&mut stream).await {
                    if let Ok(event) = serde_json::from_slice(&payload) {
                        emit(event);
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// Starts `ahab terminal-host` apart from the daemon, so it outlives it: in
/// a unit of its own under systemd (restarting Ahab's service would end
/// everything in its group otherwise), else detached through the shell.
fn start_host(socket: &Path, executable: Option<&Path>) -> Result<(), DaemonError> {
    let executable = match executable {
        Some(path) => path.to_path_buf(),
        None => std::env::current_exe().map_err(|error| {
            fail(
                "terminal_host_unavailable",
                format!("cannot find ahab: {error}"),
            )
        })?,
    };
    let socket = socket.to_string_lossy().into_owned();
    let exe = executable.to_string_lossy().into_owned();
    let under_systemd = std::env::var_os("INVOCATION_ID").is_some();
    if under_systemd {
        let started = std::process::Command::new("systemd-run")
            .args(["--user", "--quiet", "--collect", "--unit"])
            .arg(format!("ahab-terminal-host-{}", Uuid::now_v7().simple()))
            .args([&exe, "terminal-host", "--socket", &socket])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if started.is_ok_and(|status| status.success()) {
            return Ok(());
        }
    }
    // The shell starts it in the background and exits: the host is no
    // child of the daemon's.
    let quote = |text: &str| format!("'{}'", text.replace('\'', r"'\''"));
    let line = format!(
        "nohup {} terminal-host --socket {} >/dev/null 2>&1 &",
        quote(&exe),
        quote(&socket)
    );
    std::process::Command::new("/bin/sh")
        .args(["-c", &line])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|error| {
            fail(
                "terminal_host_unavailable",
                format!("cannot start the terminal host: {error}"),
            )
        })
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ask(socket: &Path, request: Value) -> (Value, UnixStream) {
        let mut stream = UnixStream::connect(socket).await.unwrap();
        write_frame(&mut stream, kind::REQUEST, request.to_string().as_bytes())
            .await
            .unwrap();
        let (_, payload) = read_frame(&mut stream).await.unwrap().unwrap();
        (serde_json::from_slice(&payload).unwrap(), stream)
    }

    /// Reads output until `wanted` shows up (or fails after five seconds).
    async fn until(stream: &mut UnixStream, wanted: &str) -> String {
        let mut seen = String::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !seen.contains(wanted) {
            let frame = tokio::time::timeout_at(deadline, read_frame(stream))
                .await
                .unwrap_or_else(|_| panic!("no {wanted:?} in {seen:?}"))
                .unwrap()
                .unwrap();
            if frame.0 == kind::OUTPUT {
                seen.push_str(&String::from_utf8_lossy(&frame.1));
            }
        }
        seen
    }

    #[test]
    fn overlapping_attachments_share_a_snapshot_until_the_screen_changes() {
        let mut screen = Screen::new(40, 5);
        screen.process(b"old");
        let first = screen.snapshot();
        assert!(Arc::ptr_eq(&first, &screen.snapshot()));
        screen.process(b" new");
        let latest = screen.snapshot();
        assert!(!Arc::ptr_eq(&first, &latest));
        assert!(String::from_utf8_lossy(&latest).contains("old new"));
    }

    #[test]
    fn archived_terminal_rows_are_replayed_and_obey_a_split_scrollback_clear() {
        let mut screen = Screen::new(40, 5);
        screen.process(b"oldest kept line\r\n");
        for _ in 0..SCROLLBACK + 20 {
            screen.process(b"more output\r\n");
        }
        assert!(String::from_utf8_lossy(&screen.snapshot()).contains("oldest kept line"));
        screen.process(b"\x1b[");
        screen.process(b"3Jafter clear");
        let snapshot = screen.snapshot();
        let shown = String::from_utf8_lossy(&snapshot);
        assert!(!shown.contains("oldest kept line"));
        assert!(shown.contains("after clear"));
    }

    #[test]
    fn a_snapshot_redraws_the_screen_and_its_history() {
        let mut screen = Screen::new(20, 4);
        for line in 1..=9 {
            screen.process(format!("line {line}\r\n").as_bytes());
        }
        screen.process(b"\x1b]2;building\x07$ ");
        assert_eq!(
            screen.parser.callbacks_mut().title.take().as_deref(),
            Some("building")
        );

        let mut copy = Screen::new(20, 4);
        copy.process(&screen.snapshot());
        assert_eq!(
            copy.parser.screen().contents(),
            screen.parser.screen().contents(),
            "the same screen"
        );
        assert_eq!(
            copy.parser.screen().cursor_position(),
            screen.parser.screen().cursor_position()
        );
        // The history before it is in the copy's scrollback.
        copy.parser.screen_mut().set_scrollback(usize::MAX);
        assert!(copy.parser.screen().contents().contains("line 1"));
    }

    #[tokio::test]
    async fn a_terminal_is_shared_by_its_attachments_and_kept_after_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join(SOCKET);
        let serving = socket.clone();
        tokio::spawn(async move { serve_host(&serving).await });
        for _ in 0..50 {
            if UnixStream::connect(&socket).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let id = Uuid::now_v7();
        let spec = Spec {
            cwd: dir.path().to_path_buf(),
            command: Some("cat".to_owned()),
            env: HashMap::new(),
        };
        let (reply, _) = ask(
            &socket,
            json!({ "op": "create", "id": id, "spec": spec, "cols": 80, "rows": 24 }),
        )
        .await;
        assert_eq!(reply, json!({}));

        // Two attachments: what one types, both see.
        let (_, mut first) = ask(&socket, json!({ "op": "attach", "id": id })).await;
        let (_, mut second) = ask(&socket, json!({ "op": "attach", "id": id })).await;
        write_frame(&mut first, kind::INPUT, b"hello\n")
            .await
            .unwrap();
        until(&mut first, "hello").await;
        until(&mut second, "hello").await;

        // A later attachment sees what happened before it.
        let (_, mut late) = ask(&socket, json!({ "op": "attach", "id": id })).await;
        until(&mut late, "hello").await;

        // Ended, it stays listed with its exit code.
        let (watch_reply, mut watching) = ask(&socket, json!({ "op": "watch" })).await;
        assert_eq!(watch_reply, json!({}));
        ask(&socket, json!({ "op": "kill", "id": id })).await;
        let (frame, payload) = read_frame(&mut watching).await.unwrap().unwrap();
        assert_eq!(frame, kind::EVENT);
        let event: Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(event["type"], "exited");
        let (list, _) = ask(&socket, json!({ "op": "list" })).await;
        assert!(list["terminals"][0]["exited"].is_i64());

        let (removed, _) = ask(&socket, json!({ "op": "remove", "id": id })).await;
        assert_eq!(removed, json!({}));
        let (list, _) = ask(&socket, json!({ "op": "list" })).await;
        assert_eq!(list["terminals"], json!([]));
    }
}
