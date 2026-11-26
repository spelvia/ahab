//! `openapi.json` is generated from the code and committed. This test fails
//! when it is out of date; `make openapi` (`UPDATE_OPENAPI=1`) rewrites it.

use std::fs;
use std::path::Path;

#[test]
fn the_openapi_document_is_current() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi.json");
    let generated = format!(
        "{}\n",
        ahab::server::router::openapi().to_pretty_json().unwrap()
    );
    if std::env::var_os("UPDATE_OPENAPI").is_some() {
        fs::write(&path, &generated).unwrap();
        return;
    }
    let committed = fs::read_to_string(&path).unwrap();
    assert!(
        committed == generated,
        "openapi.json is out of date: run `make openapi`"
    );
}
