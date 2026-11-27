//! A machine's resources, asked of its daemon each time.

use reqwest::StatusCode;

use ahab::modules::machines::MachineKey;

use crate::support::{FakeDaemon, TestServer};

#[tokio::test]
async fn a_machine_reports_its_resources_each_time_it_is_asked() {
    let server = TestServer::start().await;
    let key = MachineKey::from_secret([6; 32]);
    let machine = server.local_machine(&key, "atlas").await;
    let daemon = FakeDaemon::connect(&server, &key).await;
    let path = format!("/machines/{}/metrics", machine.id);

    let (status, metrics) = server.get(&path).await;
    assert_eq!(status, StatusCode::OK, "{metrics}");
    assert_eq!(metrics["cpu"]["cores"].as_array().unwrap().len(), 2);
    assert_eq!(metrics["processes"][0]["ahab"], true);
    server.get(&path).await;
    assert_eq!(daemon.asked_for("machine.metrics").len(), 2);

    daemon.disconnect();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let (status, problem) = server.get(&path).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{problem}");
}
