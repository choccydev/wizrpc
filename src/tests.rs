use crate::{Client, WizRPCRequest};

#[tokio::test]
async fn test_client_new() {
    Client::default().await.unwrap();
}
#[tokio::test]
async fn test_ping() {
    let client = Client::default().await.unwrap();

    client
        .register("Office".to_string(), "office.wiz.local".to_string())
        .await
        .unwrap();

    client
        .register("Room".to_string(), "room.wiz.local".to_string())
        .await
        .unwrap();

    client.ping("Office".to_string()).await.unwrap();
    client.ping("Room".to_string()).await.unwrap();
}

#[tokio::test]
async fn test_ping_5() {
    let client = Client::default().await.unwrap();

    client
        .register("Office".to_string(), "office.wiz.local".to_string())
        .await
        .unwrap();

    let amount = 0..5;

    for _ in amount {
        client.ping("Office".to_string()).await.unwrap();
    }
}

#[tokio::test]
async fn test_ping_25() {
    let client = Client::default().await.unwrap();

    client
        .register("Office".to_string(), "office.wiz.local".to_string())
        .await
        .unwrap();

    let amount = 0..25;

    for _ in amount {
        client.ping("Office".to_string()).await.unwrap();
    }
}

#[tokio::test]
async fn test_send() {
    let client = Client::default().await.unwrap();

    client
        .register("Office".to_string(), "office.wiz.local".to_string())
        .await
        .unwrap();

    let request1 = WizRPCRequest::new(
        "Office".to_string(),
        crate::MethodNames::GetSystemConfig,
        None,
    );

    let request2 = WizRPCRequest::new("Office".to_string(), crate::MethodNames::Reboot, None);

    let module = client
        .send(request1)
        .await
        .unwrap()
        .result
        .unwrap()
        .module_name
        .unwrap();

    let reboot = client
        .send(request2)
        .await
        .unwrap()
        .result
        .unwrap()
        .success
        .unwrap();

    assert_eq!(module.as_str(), "ESP01_SHRGB1C_31");
    assert_eq!(reboot, true);
}

// #[tokio::test]
// async fn test_client_discover() {
//     let mut client = Client::default().await.unwrap();

//     client.discover().await.unwrap();

//     assert_ne!(client.devices().unwrap().len(), 0);
// }
