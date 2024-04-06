use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use bip39::Mnemonic;
use tokio::sync::Mutex;
use tonic::{transport::Server, Request, Response, Status};

use key_share::coordinator_server::{Coordinator, CoordinatorServer};
use key_share::{AddKeyReply, AddKeyRequest, AddMnemonicReply, AddMnemonicRequest, KeyListReply};

const SHAMIR_SHARES: usize = 3;
const SHAMIR_THRESHOLD: usize = 2;

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


impl MyCoordinator {

    async fn add_share(&self, key_hex: String, index: u32) -> Result<String, Status> {
        let mut shares = self.key_shares.lock().await;

        if shares.len() >= SHAMIR_SHARES {
            return Ok("Enough key shares have already been added.".to_string());
        }

        let new_key_share = KeyShare { key_hex, index };

        // Check for duplicates
        if shares.iter().any(|ks| ks.key_hex == new_key_share.key_hex || ks.index == new_key_share.index) {
            return Ok("Key already exists.".to_string());
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

            let seed_path = env::var("SEED_PATH").unwrap_or_else(|_| "/home/vls/.lightning-signer/testnet".into());
            let seed_file_name = env::var("SEED_FILE_NAME").unwrap_or_else(|_| "node.seed".into());
            let seed_file = format!("{}/{}", seed_path, seed_file_name);

            let written = write_file_if_not_exists(&seed_file, &seed_content);

            message += " and secret recovered.";

            message.push_str(if written {
                " Seed written to file."
            } else {
                " Seed file already exists."
            });
        }

        Ok(message)
    }

}

#[tonic::async_trait]
impl Coordinator for MyCoordinator {

    async fn add_key(
        &self,
        request: Request<AddKeyRequest>,
    ) -> Result<Response<AddKeyReply>, Status> {

        let request_inner = request.into_inner();
        let key_hex = request_inner.keyhex;
        let index = request_inner.index;

        let message = self.add_share(key_hex, index).await?;

        Ok(Response::new(AddKeyReply { message }))
    }

    async fn add_mnemonic(
        &self,
        request: Request<AddMnemonicRequest>,
    ) -> Result<Response<AddMnemonicReply>, Status> {

        let request_inner = request.into_inner();
        let mnemonic_str = request_inner.mnemonic;
        let index = request_inner.index;
        let mnemonic = Mnemonic::parse(&mnemonic_str).unwrap();
        let key_hex = hex::encode(mnemonic.to_entropy());

        let message = self.add_share(key_hex, index).await?;

        Ok(Response::new(AddMnemonicReply { message }))
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
