//! Health Check Demo Application

use backbone_health::SimpleHealthServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run the health server demo
    SimpleHealthServer::run_demo().await.map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}