use super::ipc::*;

#[test]
fn serialize_subscribe_request() {
    let req = TrayRequest::subscribe();
    let json = serde_json::to_string(&req).unwrap();
    assert_eq!(json, r#"{"v":1,"cmd":"subscribe"}"#);
}

#[test]
fn serialize_get_pixmap_request() {
    let req = TrayRequest::get_pixmap("org.example.App", 22);
    let v: serde_json::Value = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
    assert_eq!(v["v"], 1);
    assert_eq!(v["cmd"], "get_pixmap");
    assert_eq!(v["app_id"], "org.example.App");
    assert_eq!(v["size"], 22);
}

#[test]
fn deserialize_event_response() {
    let json = r#"{"v":1,"type":"event","event":{"kind":"update","items":[{"app_id":"org.example.App","title":"Example","status":"Active","icon_handle":"example-app"}]}}"#;
    let resp: TrayResponse = serde_json::from_str(json).unwrap();
    let TrayResponse::Ok(TrayOk::Event { event }) = resp else {
        panic!("expected Ok(Event)");
    };
    assert_eq!(event.kind, TrayEventKind::Update);
    assert_eq!(event.items.len(), 1);
    let item = &event.items[0];
    assert_eq!(item.app_id, "org.example.App");
    assert_eq!(item.title.as_deref(), Some("Example"));
    assert_eq!(item.status, TrayItemStatus::Active);
    assert_eq!(item.icon_handle.as_deref(), Some("example-app"));
}

#[test]
fn deserialize_event_omits_optional_fields() {
    let json = r#"{"v":1,"type":"event","event":{"kind":"update","items":[{"app_id":"x","status":"Passive"}]}}"#;
    let resp: TrayResponse = serde_json::from_str(json).unwrap();
    let TrayResponse::Ok(TrayOk::Event { event }) = resp else {
        panic!("expected Ok(Event)");
    };
    let item = &event.items[0];
    assert!(item.title.is_none());
    assert!(item.icon_handle.is_none());
    assert_eq!(item.status, TrayItemStatus::Passive);
}

#[test]
fn deserialize_pixmap_response() {
    let json = r#"{"v":1,"type":"pixmap","app_id":"org.example.App","size":22,"width":22,"height":22,"data":"AAAA"}"#;
    let resp: TrayResponse = serde_json::from_str(json).unwrap();
    let TrayResponse::Ok(TrayOk::Pixmap {
        app_id,
        size,
        width,
        height,
        data,
    }) = resp
    else {
        panic!("expected Ok(Pixmap)");
    };
    assert_eq!(app_id, "org.example.App");
    assert_eq!(size, 22);
    assert_eq!(width, 22);
    assert_eq!(height, 22);
    assert_eq!(data, "AAAA");
}

#[test]
fn deserialize_error_response() {
    let json = r#"{"v":1,"error":{"code":"NOT_FOUND","message":"app_id not registered"}}"#;
    let resp: TrayResponse = serde_json::from_str(json).unwrap();
    let TrayResponse::Err(env) = resp else {
        panic!("expected Err");
    };
    assert_eq!(env.error.code, "NOT_FOUND");
    assert_eq!(env.error.message, "app_id not registered");
}

#[test]
fn needs_attention_status() {
    let json = r#"{"app_id":"x","status":"NeedsAttention"}"#;
    let item: MinimalTrayItem = serde_json::from_str(json).unwrap();
    assert_eq!(item.status, TrayItemStatus::NeedsAttention);
}

#[test]
fn unknown_event_kind_is_tolerated() {
    let json = r#"{"v":1,"type":"event","event":{"kind":"future_kind","items":[]}}"#;
    let resp: TrayResponse = serde_json::from_str(json).unwrap();
    let TrayResponse::Ok(TrayOk::Event { event }) = resp else {
        panic!("expected Ok(Event)");
    };
    assert_eq!(event.kind, TrayEventKind::Unknown);
}
