//! Channels: a browser's WebSocket to a plugin's stream on its machine.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use super::{echo_copy, local_daemon};
use crate::support::TestServer;

#[tokio::test]
async fn a_channel_carries_bytes_both_ways_and_ends_from_either_side() {
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
    let base = server.url.replacen("http", "ws", 1);
    let params = "%7B%22prefix%22%3A%22%3E%20%22%7D"; // {"prefix":"> "}
    let url = format!("{base}/api/v1/plugins/echo/channels/lines?params={params}");

    let mut socket = server.ws(url.as_str()).await.unwrap();
    socket
        .send(Message::Binary(b"hi".to_vec().into()))
        .await
        .unwrap();
    let echoed = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(echoed, Message::Binary(b"> hi".to_vec().into()));

    // The plugin ends it, with a reason.
    socket
        .send(Message::Binary(b"bye".to_vec().into()))
        .await
        .unwrap();
    let ended = tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(
        ended,
        Message::Text(
            json!({ "type": "end", "reason": "Echo says bye." })
                .to_string()
                .into()
        )
    );

    // The browser ends one: the plugin hears of it.
    let mut other = server.ws(url.as_str()).await.unwrap();
    other
        .send(Message::Binary(b"x".to_vec().into()))
        .await
        .unwrap();
    other.next().await;
    other.close(None).await.unwrap();
    for _ in 0..100 {
        let (_, logs) = server
            .get(&format!("/plugins/echo/logs?machine={}", machine.id))
            .await;
        if logs.to_string().contains("lines closed") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the plugin did not hear the channel close");
}

#[tokio::test]
async fn an_unknown_channel_is_refused() {
    let server = TestServer::start().await;
    let (_machine, _daemon) = local_daemon(&server, None).await;
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
    let base = server.url.replacen("http", "ws", 1);
    let refused = server
        .ws(&format!("{base}/api/v1/plugins/echo/channels/nope"))
        .await;
    assert!(refused.is_err(), "no such channel");
}
