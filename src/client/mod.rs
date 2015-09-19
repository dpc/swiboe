// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

#![allow(deprecated)]

use ::error::Result;
use ::ipc;
use ::plugin_core::NewRpcRequest;
use std::net::{self, TcpStream};
use unix_socket::UnixStream;
use serde;
use std::io;
use std::path;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

pub trait RpcCaller {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<::client::rpc::client::Context>;
}

pub struct Client {
    rpc_loop_commands: rpc_loop::CommandSender,
    rpc_loop_thread: Option<thread::JoinHandle<()>>,
    read_thread: Option<thread::JoinHandle<()>>,
    write_thread: Option<thread::JoinHandle<()>>,
    shutdown_socket_func: Box<Fn() -> ()>,
}


impl Client {
    pub fn connect_unix(socket_name: &path::Path) -> Result<Self> {
        let writer_stream = try!(UnixStream::connect(&socket_name));
        let reader_stream = try!(writer_stream.try_clone());
        let shutdown_stream = try!(writer_stream.try_clone());
        Ok(Client::common_connect(reader_stream, writer_stream, Box::new(move || {
            let _ = shutdown_stream.shutdown(net::Shutdown::Both);
        })))
    }

    pub fn connect_tcp(address: &net::SocketAddr) -> Result<Self> {
        let writer_stream = try!(TcpStream::connect(address));
        try!(writer_stream.set_write_timeout(None));
        let reader_stream = try!(writer_stream.try_clone());
        try!(reader_stream.set_read_timeout(None));
        let shutdown_stream = try!(writer_stream.try_clone());
        Ok(Client::common_connect(reader_stream, writer_stream, Box::new(move || {
            let _ = shutdown_stream.shutdown(net::Shutdown::Both);
        })))
    }

    fn common_connect<Reader: io::Read + Send + 'static, Writer: io::Write + Send + 'static>(reader_stream: Reader, writer_stream: Writer, shutdown_func: Box<Fn() -> ()>) -> Self {
        let (commands_tx, commands_rx) = mpsc::channel();
        let (send_tx, send_rx) = mpsc::channel::<ipc::Message>();

        let reader_commands_tx = commands_tx.clone();
        let read_thread = thread::spawn(move || {
            let mut reader = ipc::Reader::new(reader_stream);
            while let Ok(message) = reader.read_one_message() {
                let command = rpc_loop::Command::Received(message);
                if reader_commands_tx.send(command).is_err() {
                    break;
                }
            };
        });

        let write_thread = thread::spawn(move || {
            let mut writer = ipc::Writer::new(writer_stream);
            while let Ok(message) = send_rx.recv() {
                if writer.write_message(&message).is_err() {
                    break;
                }
            }
        });

        Client {
            rpc_loop_commands: commands_tx.clone(),
            rpc_loop_thread: Some(rpc_loop::spawn(commands_rx, commands_tx, send_tx)),
            read_thread: Some(read_thread),
            write_thread: Some(write_thread),
            shutdown_socket_func: shutdown_func,
        }
    }

    pub fn new_rpc(&mut self, name: &str, rpc: Box<rpc::server::Rpc>) -> Result<()> {
        let mut new_rpc = try!(self.call("core.new_rpc", &NewRpcRequest {
            priority: rpc.priority(),
            name: name.into(),
        }));
        let result = new_rpc.wait();

        if !result.is_ok() {
            return Err(result.unwrap_err().into());
        }

        self.rpc_loop_commands.send(rpc_loop::Command::NewRpc(name.into(), rpc)).expect("NewRpc");
        Ok(())
    }

    pub fn clone(&self) -> Result<ThinClient> {
        Ok(ThinClient {
            rpc_loop_commands: Mutex::new(self.rpc_loop_commands.clone()),
        })
    }
}

impl RpcCaller for Client {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<rpc::client::Context> {
        rpc::client::Context::new(self.rpc_loop_commands.clone(), function, args)
    }
}


impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.rpc_loop_commands.send(rpc_loop::Command::Quit);
        if let Some(thread) = self.rpc_loop_thread.take() {
            thread.join().expect("Joining rpc_loop_thread failed.");
        }

        (self.shutdown_socket_func)();

        if let Some(thread) = self.write_thread.take() {
            thread.join().expect("Joining write_thread failed.");
        }
        if let Some(thread) = self.read_thread.take() {
            thread.join().expect("Joining read_thread failed.");
        }
    }
}

pub struct ThinClient {
    rpc_loop_commands: Mutex<rpc_loop::CommandSender>,
}

// NOCOM(#sirver): figure out the difference between a Sender, an Context and come up with better
// names.
impl ThinClient {
    pub fn clone(&self) -> Self {
        let commands = {
            let commands = self.rpc_loop_commands.lock().unwrap();
            commands.clone()
        };
        ThinClient {
            rpc_loop_commands: Mutex::new(commands),
        }
    }
}

impl RpcCaller for ThinClient {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<rpc::client::Context> {
        let commands = {
            let commands = self.rpc_loop_commands.lock().unwrap();
            commands.clone()
        };
        rpc::client::Context::new(commands, function, args)
    }
}


mod rpc_loop;

pub mod rpc;
