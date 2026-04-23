use std::{
    io,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};

use rumqttc::{Client, ConnectReturnCode, Event, Incoming, MqttOptions, QoS};

use crate::{infrastructure::config, shared::identifier as sn};

pub trait MqttClient {
    fn conn(&mut self) -> io::Result<()>; // Connect to the MQTT server.  This function is called once for each client.  It should return an error if the connection fails.

    fn publish(&mut self, topic: &str, data: &str) -> io::Result<()>;

    fn subscribe(&mut self, topics: Vec<String>) -> io::Result<()>;

    fn recv(&self) -> Result<(String, String), mpsc::TryRecvError>; // Receive a message from the server.  This function is called once per message.  It should return an error if the message fails to be received

    fn disconn(&self) -> io::Result<()>;
}

pub struct NormallyMqttClient {
    mqtt_receiver: Receiver<(String, String)>,

    mqtt_conn_receiver: Receiver<bool>,

    mqtt: Client,
}

impl Default for NormallyMqttClient {
    fn default() -> Self {
        Self::new()
    }
}

impl NormallyMqttClient {
    pub fn new() -> Self {
        let (mqtt_sender, mqtt_receiver) = mpsc::channel::<(String, String)>();

        let (mqtt_conn_sender, mqtt_conn_receiver) = mpsc::channel::<bool>();

        let cfg = config::get();
        let host = cfg.mqtt.primary.host.clone();
        let port = cfg.mqtt.primary.port;
        let username = cfg.mqtt.primary.username.clone().unwrap_or_else(|| "admin".to_string());
        let password = cfg.mqtt.primary.password.clone().unwrap_or_else(|| "password".to_string());

        let sn = sn::get_sn();

        tracing::error!("mqtt init  {}:{},{},{}", host, port, username, password);

        let mut options = MqttOptions::new(format!("edge_{}", sn), host, port);

        options.set_keep_alive(Duration::from_secs(30)).set_credentials(username, password);

        let (client, mut conn) = Client::new(options, 100);

        thread::spawn(move || {
            for (i, notification) in conn.iter().enumerate() {
                match notification {
                    Ok(notif) => {
                        tracing::error!("{}. Notification = {:?}", i, notif);

                        if let Event::Incoming(inc) = notif {
                            match inc {
                                Incoming::Publish(dt) => {
                                    if let Err(e) = mqtt_sender.send((
                                        dt.topic,
                                        String::from_utf8_lossy(&dt.payload).to_string(),
                                    )) {
                                        tracing::error!(
                                            "Failed to send MQTT message to channel: {}",
                                            e
                                        );
                                    }
                                }

                                Incoming::ConnAck(conn) => {
                                    //on_connected(conn.code);

                                    tracing::error!("conn ack code {:?}", conn.code);

                                    if conn.code == ConnectReturnCode::Success
                                        && let Err(e) = mqtt_conn_sender.send(true) {
                                            tracing::error!(
                                                "Failed to send connection success signal: {}",
                                                e
                                            );
                                        }
                                }

                                _ => {}
                            }
                        };
                    }

                    Err(error) => {
                        tracing::error!("conn err {}", error);

                        if let Err(e) = mqtt_conn_sender.send(false) {
                            tracing::error!("Failed to send connection failure signal: {}", e);
                        }
                    }
                }
            }
        });

        Self { mqtt: client, mqtt_conn_receiver, mqtt_receiver }
    }
}

impl MqttClient for NormallyMqttClient {
    fn conn(&mut self) -> io::Result<()> {
        match self.mqtt_conn_receiver.recv() {
            Err(e) => Err(io::Error::other(format!("conn has error,{e}"))),

            Ok(rs) => {
                if rs {
                    return Ok(());
                }

                Err(io::Error::other("conn init fault".to_string()))
            }
        }
    }

    fn publish(&mut self, topic: &str, data: &str) -> io::Result<()> {
        match self.mqtt.publish(topic, QoS::AtMostOnce, false, data.to_string().as_bytes()) {
            Err(e) => Err(io::Error::other(format!("publish has error,{e}"))),

            Ok(()) => Ok(()),
        }
    }

    fn subscribe(&mut self, topics: Vec<String>) -> io::Result<()> {
        for topic in topics {
            if let Err(e) = self.mqtt.subscribe(topic, QoS::AtMostOnce) {
                return Err(io::Error::other(format!("subscribe has error {e}")));
            }
        }

        Ok(())
    }

    fn recv(&self) -> Result<(String, String), mpsc::TryRecvError> {
        self.mqtt_receiver.try_recv()
    }

    fn disconn(&self) -> io::Result<()> {
        Ok(())
    }
}
