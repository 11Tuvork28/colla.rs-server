use std::{fs, net::Ipv4Addr};
use crate::messages::Message;
use crate::commands::Command;
use tokio::sync::broadcast::{Receiver, Sender};
use serde::{Deserialize, Serialize};
pub struct State{
    pub rx_collar: Receiver<Command>,
    pub tx_collar: Sender<Message>,
    pub tx_requester: Sender<Command>,
    pub rx_requester: Receiver<Message>,
    pub key_collar: String,
    pub key_him: String,
    pub rx_collar_write: Receiver<Message>,
    pub tx_collar_read: Sender<Message>,
}


#[derive(Debug,Serialize, Deserialize)]
pub struct Config{
    pub key_collar: String,
    pub key_him: String,
    pub listen_on_port: i16,
    pub interface_ip: Ipv4Addr,
    pub log_level: String
}

impl Config {
    /// Loads the configuration and parses it into a Config object, panics if the configuration file is invalid or missing.
    pub fn new() -> Config {
        let loaded_config = fs::read_to_string("config.yaml").unwrap();
        serde_yaml::from_str(&loaded_config).unwrap()
    }
}