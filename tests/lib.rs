extern crate serde;
extern crate switchboard;
extern crate uuid;

use serde::json;
use std::env;
use std::path::{PathBuf};
use switchboard::client::{self, RemoteProcedure, Client};
use switchboard::ipc::RpcResultKind;
use switchboard::server::Server;
use uuid::Uuid;

// NOCOM(#sirver): use the name switchboard everywhere.

struct TestServer {
    server: Option<Server>,
}

impl TestServer {
    fn new() -> (Self, PathBuf) {
        let socket_name = temporary_socket_name();
        let server = Server::launch(&socket_name);

        (TestServer { server: Some(server), }, socket_name)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.take().unwrap().shutdown();
    }
}

fn temporary_socket_name() -> PathBuf {
    let mut dir = env::temp_dir();
    dir.push(format!("{}.socket", Uuid::new_v4().to_string()));
    dir
}

#[test]
fn shutdown_server_with_clients_connected() {
    let socket_name = temporary_socket_name();
    let mut server = Server::launch(&socket_name);

    let _client = Client::connect(&socket_name.to_string_lossy());

    server.shutdown();
}

#[test]
fn shutdown_server_with_no_clients_connected() {
    let (_server, socket_name) = TestServer::new();

    let _client = Client::connect(&socket_name.to_string_lossy());
}

#[test]
fn broadcast_works() {
    let (_server, socket_name) = TestServer::new();

    let client1 = Client::connect(&socket_name.to_string_lossy());
    let client2 = Client::connect(&socket_name.to_string_lossy());

    let test_msg = json::builder::ObjectBuilder::new()
        .insert("blub".into(), "blah")
        .unwrap();

    let rpc = client1.call("core.broadcast", &test_msg);
    assert_eq!(rpc.wait().unwrap(), RpcResultKind::Ok);

    let broadcast_msg = client1.recv().unwrap();
    assert_eq!(test_msg, broadcast_msg);

    let broadcast_msg = client2.recv().unwrap();
    assert_eq!(test_msg, broadcast_msg);
}

// NOCOM(#sirver): this test is not yet done.
#[test]
fn register_function() {
    let (_server, socket_name) = TestServer::new();

    let client1 = Client::connect(&socket_name.to_string_lossy());
    let client2 = Client::connect(&socket_name.to_string_lossy());

    struct TestCall {
        client_handle: client::ClientHandle,
    };

    impl RemoteProcedure for TestCall {
        // NOCOM(#sirver): the client handle should be passed in.
        fn call(&mut self, args: json::Value) -> RpcResultKind {
            let rpc = self.client_handle.call("core.broadcast", &args);
            rpc.wait().unwrap()
        }
    }
    let client_handle = client1.client_handle();
    client1.register_function("testclient.test", Box::new(TestCall {
        client_handle: client_handle,
    }));


    let test_msg = json::builder::ObjectBuilder::new()
        .insert("blub".into(), "blah")
        .unwrap();

    let rpc = client2.call("testclient.test", &test_msg);
    assert_eq!(rpc.wait().unwrap(), RpcResultKind::Ok);

    let broadcast_msg = client1.recv().unwrap();
    assert_eq!(test_msg, broadcast_msg);

    let broadcast_msg = client2.recv().unwrap();
    assert_eq!(test_msg, broadcast_msg);
}

// NOCOM(#sirver): test is needed.
// #[test]
// fn waiting_for_call_does_not_mean_you_miss_data() {
    // let (_server, socket_name) = TestServer::new();

    // let client1 = Client::connect(&socket_name.to_string_lossy());
    // let client2 = Client::connect(&socket_name.to_string_lossy());

    // let test_msg = json::builder::ObjectBuilder::new()
        // .insert("blub".into(), "blah")
        // .unwrap();

    // let rpc = client1.call("core.broadcast", &test_msg);
    // assert_eq!(rpc.wait().unwrap(), RpcResultKind::Ok);

    // let broadcast_msg = client1.recv().unwrap();
    // assert_eq!(test_msg, broadcast_msg);

    // let broadcast_msg = client2.recv().unwrap();
    // assert_eq!(test_msg, broadcast_msg);
// }