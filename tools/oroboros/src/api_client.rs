// Minimal hand-written "generated" client mapping exposing a couple of CouchDB endpoints
// This acts as a placeholder for a Swagger/OpenAPI-generated client. It provides small
// convenience methods used by the rest of the binary and tests.

use std::sync::Arc;
use crate::couch_client::CouchClient;

pub struct GeneratedClient {
    couch: Arc<CouchClient>,
}

impl GeneratedClient {
    pub fn new(couch: Arc<CouchClient>) -> Self { Self { couch } }

    // Convenience: ensure database exists (no-op in this minimal mapping)
    pub async fn ensure_db(&self) -> anyhow::Result<()> {
        // Left as a no-op for now; a real generated client would call PUT /{db}
        Ok(())
    }

    // Map an endpoint to create a document by id
    pub async fn create_doc(&self, docid: &str) -> anyhow::Result<String> {
        self.couch.create_doc_if_missing(docid).await
    }

    // Map endpoint to upload attachment
    pub async fn put_attachment(&self, docid: &str, name: &str, data: Vec<u8>, content_type: &str, rev_opt: Option<String>) -> anyhow::Result<String> {
        self.couch.put_attachment(docid, name, data, content_type, rev_opt).await
    }
}
