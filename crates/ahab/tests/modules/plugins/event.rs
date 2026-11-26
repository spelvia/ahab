//! Events: plugins publish and hear them; the web app gets them as a stream.

use std::time::Duration;

use serde_json::{Value, json};

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

/// The next server-sent event's data, within five seconds.
async fn next_event(response: &mut reqwest::Response, buffer: &mut String) -> Value {
    let read = async {
        loop {
            if let Some(end) = buffer.find("\n\n") {
                let block: String = buffer.drain(..end + 2).collect();
                if let Some(data) = block.lines().find_map(|line| line.strip_prefix("data: ")) {
                    return serde_json::from_str(data).unwrap();
                }
                continue;
            }
            let Some(chunk) = response.chunk().await.unwrap() else {
                panic!("the stream ended");
            };
            buffer.push_str(&String::from_utf8_lossy(&chunk));
        }
    };
    let Ok(event) = tokio::time::timeout(Duration::from_secs(5), read).await else {
        panic!("no event within five seconds");
    };
    event
}

#[tokio::test]
async fn a_plugins_event_reaches_the_web_app_and_the_plugins_that_listen() {
    let server = TestServer::start().await;
    let (machine, _daemon) = local_daemon(&server, None).await;
    let copy = echo_copy();
    server
        .post(
            "/plugins",
            json!({ "source": "folder", "location": copy.path() }),
        )
        .await;
    server.post("/plugins/echo/enable", json!({})).await;
    for _ in 0..200 {
        let (_, states) = server.get("/plugin-states").await;
        if states["items"][0]["state"] == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let mut stream = server
        .http()
        .get(format!(
            "{}/api/v1/plugin-events?names=echo.said",
            server.url
        ))
        .send()
        .await
        .unwrap();
    let mut buffer = String::new();
    server
        .post(
            "/plugins/echo/commands/say",
            json!({ "input": { "text": "hello" } }),
        )
        .await;
    let heard = next_event(&mut stream, &mut buffer).await;
    assert_eq!(
        heard,
        json!({ "name": "echo.said", "data": { "text": "hello" } })
    );

    // Echo listens to its own event, through Ahab.
    for _ in 0..100 {
        let (_, logs) = server
            .get(&format!("/plugins/echo/logs?machine={}", machine.id))
            .await;
        if logs.to_string().contains("heard echo.said") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("echo did not hear its event");
}
