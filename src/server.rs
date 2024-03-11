use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};

use key_share::coordinator_server::{Coordinator, CoordinatorServer};
use key_share::{KeyListReply, AddKeyRequest, AddKeyReply};

use shamir_secret_sharing::num_bigint::BigInt;
use shamir_secret_sharing::ShamirSecretSharing as SSS;

const SHAMIR_SHARES: usize = 3;
const SHAMIR_THRESHOLD: usize = 2;

const CURVE: &str = "fffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2f";

const SEED_FILE: &str = "/home/node/node.seed";

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

#[derive(Debug, Default)]
pub struct MyCoordinator {
    key_shares: Arc<Mutex<Vec<String>>>,
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

        let key_hex = request.into_inner().keyhex;

        // if key_hex is already in shares, return an error
        if shares.contains(&key_hex) {
            // return Err(Status::invalid_argument("Key already exists"));
            return Ok(Response::new(key_share::AddKeyReply {
                message: "Key already exists.".to_string(), // We must use .into_inner() as the fields of gRPC requests and responses are private
            }));
        }

        // if key_hex is not in shares, add it

        shares.push(key_hex);

        let mut message = "Key added successfully".to_string();

        if shares.len() >= SHAMIR_THRESHOLD {

            let sss = SSS {
                threshold: SHAMIR_THRESHOLD,
                share_amount: SHAMIR_SHARES,
                prime: BigInt::parse_bytes(CURVE.as_bytes(), 16).unwrap()
            };

            let mut shamir_shares: Vec<(usize, BigInt)> = Vec::new();

            for share in shares.iter() {
                let secret = BigInt::parse_bytes(share.as_bytes(), 16).unwrap();
                shamir_shares.push((shares.iter().position(|x| x == share).unwrap() + 1, secret));
            }

            let recovered_secret = sss.recover(&shamir_shares);

            let seed_content = recovered_secret.to_str_radix(16);

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
            message.items.push(key_share.to_string());
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
