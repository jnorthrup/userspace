use httptest::{matchers::*, responders::*, Expectation, Server};
use httptest::all_of;
use oroboros::couch_client::{CouchClient, CouchConfig};
use oroboros::api_client::GeneratedClient;
use reqwest::Client;
use tokio;

#[tokio::test]
async fn create_doc_and_put_attachment_flow() {
    let server = Server::run();

    // Simulate GET doc -> 404
    server.expect(
        Expectation::matching(request::method_path("GET", "/testdb/doc1"))
            .times(1)
            .respond_with(status_code(404).body(r#"{"error":"not_found"}"#)),
    );

    // Simulate PUT doc (create) -> 201 with rev
    server.expect(
        Expectation::matching(all_of![request::method_path("PUT", "/testdb/doc1"),])
            .times(1)
            .respond_with(status_code(201).body(r#"{"ok":true,"id":"doc1","rev":"1-abc"}"#)),
    );

    // Simulate PUT attachment -> 201 rev 2
    server.expect(
        Expectation::matching(request::method_path("PUT", "/testdb/doc1/avatar.png"))
            .times(1)
            .respond_with(status_code(201).body(r#"{"ok":true,"id":"doc1","rev":"2-def"}"#)),
    );

    let client = Client::builder().build().unwrap();
    let cfg = CouchConfig { base: server.url("/").to_string(), db: "testdb".to_string(), user: None, pass: None, concurrency: 2, max_retries: 3 };
    let couch = CouchClient::new(client, cfg);
    let gen = GeneratedClient::new(std::sync::Arc::new(couch));

    // Ensure doc missing
    let rev = couch.get_doc_rev("doc1").await.unwrap();
    assert!(rev.is_none());

    // Create doc
    let new_rev = couch.create_doc_if_missing("doc1").await.unwrap();
    assert_eq!(new_rev, "1-abc");

    // Put attachment
    let data = b"hello".to_vec();
    let rev2 = gen.put_attachment("doc1", "avatar.png", data, "image/png", Some(new_rev)).await.unwrap();
    assert_eq!(rev2, "2-def");
}

#[tokio::test]
async fn generated_client_create_doc() {
    let server = Server::run();
    server.expect(
        Expectation::matching(request::method_path("PUT", "/testdb/special%2Fid"))
            .times(1)
            .respond_with(status_code(201).body(r#"{"ok":true,"id":"special/id","rev":"1-xyz"}"#)),
    );

    let client = Client::builder().build().unwrap();
    let cfg = CouchConfig { base: server.url("/").to_string(), db: "testdb".to_string(), user: None, pass: None, concurrency: 2, max_retries: 3 };
    let couch = CouchClient::new(client, cfg);
    let gen = GeneratedClient::new(std::sync::Arc::new(couch));

    let rev = gen.create_doc("special/id").await.unwrap();
    assert_eq!(rev, "1-xyz");
}
