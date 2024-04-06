# Shamir Secret Sharing RPC

# Running from source

`cargo run --bin keyshare-server` to start the server.

`cargo run --bin keyshare-client add-key-share 5b654db9c93b6c68dcc9d0bd2eb8730da3b796207c4105df27f7da270ae627a4 0`  to add a key share.

The command is `cargo run --bin keyshare-client add-key-share <key_share_hex> <index>`.

# Running from Dockerfile

`$ docker build --no-cache -t keyshare .` to build the image.

`docker run -d --name keyshare-server -v /home/user:/home/vls/.lightning-signer/testnet -p 50051:50051 keyshare` to run the container

`docker exec keyshare-server keyshare-client add-key-share 5b654db9c93b6c68dcc9d0bd2eb8730da3b796207c4105df27f7da270ae627a4 0` to send a key.