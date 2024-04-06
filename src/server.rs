use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};

use key_share::coordinator_server::{Coordinator, CoordinatorServer};
use key_share::{KeyListReply, AddKeyRequest, AddKeyReply};

const SHAMIR_SHARES: usize = 3;
const SHAMIR_THRESHOLD: usize = 2;

const SEED_FILE: &str = "/home/vls/.lightning-signer/testnet/node.seed";

fn write_file_if_not_exists(path: &str, content: &str) -> bool {
    let path = Path::new(path);

    // Check if the file does not exist
    if !path.exists() {
        // Open the file in write-only mode, create it if it does not exist.
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true) // This ensures the file is created only if it does not exist
            .open(path).unwrap();

        let content = content.trim_end_matches(|c| c == '\r' || c == '\n');


        // Write the content to the file
        file.write_all(content.as_bytes()).unwrap();

        return true;
    }

    false
}

pub mod key_share {
    tonic::include_proto!("keyshare"); // The string specified here must match the proto package name
}

#[derive(Debug, Default, PartialEq)]
pub struct KeyShare {
    key_hex: String,
    index: u32,
}

#[derive(Debug, Default)]
pub struct MyCoordinator {
    key_shares: Arc<Mutex<Vec<KeyShare>>>,
}


#[tonic::async_trait]
impl Coordinator for MyCoordinator {

    async fn add_key(
        &self,
        request: Request<AddKeyRequest>,
    ) -> Result<Response<AddKeyReply>, Status> {

        let mut shares = self.key_shares.lock().await;

        if shares.len() >= SHAMIR_SHARES {
            return Ok(Response::new(key_share::AddKeyReply {
                message: "Enough key shares have already been added.".to_string(), // We must use .into_inner() as the fields of gRPC requests and responses are private
            }));
        }

        let request_inner = request.into_inner();
        let key_hex = request_inner.keyhex;
        let index = request_inner.index;

        let new_key_share = KeyShare {
            key_hex: key_hex.clone(),
            index,
        };

        // Check for duplicates
        if shares.iter().any(|ks| ks.key_hex == new_key_share.key_hex || ks.index == new_key_share.index) {
            return Ok(Response::new(key_share::AddKeyReply {
                message: "Key already exists.".to_string(), // We must use .into_inner() as the fields of gRPC requests and responses are private
            }));
        } else {
            shares.push(new_key_share); // Insert the new KeyShare if no duplicates are found
        }

        let mut message = "Key added successfully".to_string();

        let mut secret_shares: Vec<Vec<u8>> = Vec::new();
        let mut indexes: Vec<usize> = Vec::new();

        if shares.len() >= SHAMIR_THRESHOLD {

            for share in shares.iter() {
                let ks = hex::decode(share.key_hex.to_string()).unwrap();
                secret_shares.push(ks);
                indexes.push(share.index as usize);
            }

            let secret = bc_shamir::recover_secret(&indexes, &secret_shares).unwrap();

            let seed_content =  hex::encode(secret);

            let written = write_file_if_not_exists(SEED_FILE, &seed_content);

            message += " and secret recovered.";

            message.push_str(if written {
                " Seed written to file."
            } else {
                " Seed file already exists."
            });

        }

        let reply = key_share::AddKeyReply {
            message
        };

        // return the reply

        Ok(Response::new(reply))
    }

    async fn list_keys(&self, _request: Request<()>) -> Result<Response<KeyListReply>, Status> {

        let mut message = KeyListReply::default();

        let shares = self.key_shares.lock().await;

        for key_share in &shares[..] {
            message.items.push(key_share.key_hex.to_string());
        }

        Ok(Response::new(message))

    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let coordinator = MyCoordinator::default();

    Server::builder()
        .add_service(CoordinatorServer::new(coordinator))
        .serve(addr)
        .await?;

    Ok(())
}
