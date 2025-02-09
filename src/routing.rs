use rosc::decoder::MTU;
use rosc::{self, OscPacket, encoder};
use serde::{Deserialize, Serialize};
use std::net::{UdpSocket, Ipv4Addr};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::{self, Receiver as bcst_Receiver, Sender as bcst_Sender};

use crate::routedbg;
use crate::{
    config::{VORAppIdentifier, VORAppStatus, VORConfig},
    vorerr::app_error,
};

pub enum RouterMsg {
    ShutdownAll,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct PacketFilter {
    pub enabled: bool,
    pub filter_bad_packets: bool,
    pub wl_enabled: bool,
    //pub wl_editing: bool,
    pub address_wl: Vec<(String, bool)>,
    pub bl_enabled: bool,
    //pub bl_editing: bool,
    pub address_bl: Vec<(String, bool)>,
}

fn route_app(
    mut rx: bcst_Receiver<Vec<u8>>,
    router_rx: Receiver<bool>,
    app_stat_tx_at: Sender<VORAppIdentifier>,
    ai: i64,
    app: VORConfig,
    debug_sender: Option<Sender<routedbg::DebugPacket>>,
    _debug_out_config: Option<routedbg::VORDebugOptions>,
) {
    let rhp = format!("{}:{}", app.app_host, app.app_port);
    //let lhp = format!("{}:{}", app.bind_host, app.bind_port);
    let sock = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx_at.send(app_error(
                ai,
                -2,
                format!("Failed to bind app UdpSocket: {}", _e),
            ));
            return; // Close app route thread because app failed to bind
        }
    };
    //println!("[*] OSC App: [{}] Route Initialized..", app.app_name);
    let _ = app_stat_tx_at.send(VORAppIdentifier {
        index: ai,
        status: VORAppStatus::Running,
    });
    //let r = router_rx.recv_timeout(std::time::Duration::from_secs(1));

    loop {
        ////println!("router_rx start");
        match router_rx.try_recv() {
            Ok(signal) => {
                ////println!("[!] signal: {}", signal);
                if signal {
                    let _ = app_stat_tx_at.send(VORAppIdentifier {
                        index: ai,
                        status: VORAppStatus::Stopped,
                    });
                    //println!("[!] Send Stopped status");
                    return;
                }
            }
            _ => { /*//println!("[!] Try recv errors")*/ }
        }
        ////println!("router_rx done");
        // Get vrc OSC buffer
        match rx.try_recv() {
            Ok(b) => {
                // Route buffer
                match sock.send_to(&b, &rhp) {
                    Ok(_bs) => {
                        if let Some(ref dbgs) = debug_sender {
                            // Try to get parsed packet
                            if let Ok(pkt) = rosc::decoder::decode_udp(&b) {
                                routedbg::send_outdbg_packet(
                                    dbgs,
                                    app.app_name.clone(),
                                    rhp.clone(),
                                    &b,
                                    Some(pkt.1),
                                );
                            } else {
                                routedbg::send_outdbg_packet(
                                    dbgs,
                                    app.app_name.clone(),
                                    rhp.clone(),
                                    &b,
                                    None,
                                );
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = app_stat_tx_at.send(app_error(
                            ai,
                            -3,
                            format!("Failed to send VRC OSC buffer to app: {}", _e),
                        ));
                    }
                }
            }
            Err(TryRecvError::Empty) => continue,
            Err(TryRecvError::Lagged(_)) => continue,
            Err(TryRecvError::Closed) => {
                // VRC OSC BUFFER CHANNEL DIED SO KILL ROUTE THREAD
                let _ = app_stat_tx_at.send(VORAppIdentifier {
                    index: ai,
                    status: VORAppStatus::Stopped,
                });

                return;
            }
        };
    }
}

async fn route_app_async(
    mut rx: bcst_Receiver<Vec<u8>>,
    router_rx: Receiver<bool>,
    app_stat_tx_at: Sender<VORAppIdentifier>,
    ai: i64,
    app: VORConfig,
    debug_sender: Option<Sender<routedbg::DebugPacket>>,
    _debug_out_config: Option<routedbg::VORDebugOptions>,
) {
    let rhp = format!("{}:{}", app.app_host, app.app_port);
    //let lhp = format!("{}:{}", app.bind_host, app.bind_port);
    let sock = match tokio::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).await {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx_at.send(app_error(
                ai,
                -2,
                format!("Failed to bind app UdpSocket: {}", _e),
            ));
            return; // Close app route thread because app failed to bind
        }
    };
    //println!("[*] OSC App: [{}] Route Initialized..", app.app_name);
    let _ = app_stat_tx_at.send(VORAppIdentifier {
        index: ai,
        status: VORAppStatus::Running,
    });
    //let r = router_rx.recv_timeout(std::time::Duration::from_secs(1));

    loop {
        ////println!("router_rx start");
        match router_rx.try_recv() {
            Ok(signal) => {
                ////println!("[!] signal: {}", signal);
                if signal {
                    let _ = app_stat_tx_at.send(VORAppIdentifier {
                        index: ai,
                        status: VORAppStatus::Stopped,
                    });
                    //println!("[!] Send Stopped status");
                    return;
                }
            }
            _ => { /*//println!("[!] Try recv errors")*/ }
        }
        ////println!("router_rx done");
        // Get vrc OSC buffer
        // route_main thread should abort this await on async runtime shutdown when threads are aborted. So don't have to worry about thread blocking with recv
        match rx.recv().await {
            Ok(b) => {
                // Route buffer

                match sock.send_to(&b, &rhp).await {
                    Ok(_bs) => {
                        if let Some(ref dbgs) = debug_sender {
                            // Try to get parsed packet
                            if let Ok(pkt) = rosc::decoder::decode_udp(&b) {
                                routedbg::send_outdbg_packet(
                                    dbgs,
                                    app.app_name.clone(),
                                    rhp.clone(),
                                    &b,
                                    Some(pkt.1),
                                );
                            } else {
                                routedbg::send_outdbg_packet(
                                    dbgs,
                                    app.app_name.clone(),
                                    rhp.clone(),
                                    &b,
                                    None,
                                );
                            }
                        }
                    }
                    Err(_e) => {
                        let _ = app_stat_tx_at.send(app_error(
                            ai,
                            -3,
                            format!("Failed to send VRC OSC buffer to app: {}", _e),
                        ));
                    }
                }
            }
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => {
                // VRC OSC BUFFER CHANNEL DIED SO KILL ROUTE THREAD
                let _ = app_stat_tx_at.send(VORAppIdentifier {
                    index: ai,
                    status: VORAppStatus::Stopped,
                });

                return;
            }
        };
    }
}

fn parse_vrc_osc(
    bcst_tx: bcst_Sender<Vec<u8>>,
    router_rx: Receiver<bool>,
    pf: PacketFilter,
    vrc_sock: UdpSocket,
    _debug_incoming_config: Option<routedbg::VORDebugOptions>,
    debug_sender: Option<Sender<routedbg::DebugPacket>>,
) {
    let pf_wl: Vec<String> = pf.address_wl.iter().map(|i| i.0.clone()).collect();
    let pf_bl: Vec<String> = pf.address_bl.iter().map(|i| i.0.clone()).collect();

    // Here sending the decoded packet's buffer instead of the UDP buffer
    // because some OSC libraries cant parse OSC packets with trailing NULL bytes.
    // However if malformed OSC packets are relayed due to PF allowing bad packets through they will be sent with a full MTU (NULL BYTES PADDED)
    let mut buf = [0u8; MTU];

    loop {
        match vrc_sock.recv_from(&mut buf) {
            Ok((br, address)) => {
                if br <= 0 {
                    // If got bytes send them to routers otherwise restart loop
                    continue;
                } else {
                    // Packet Filtering

                    if pf.enabled {
                        // PF enabled
                        if pf.wl_enabled {
                            // Whitelist
                            match rosc::decoder::decode_udp(&buf) {
                                Ok(ref pkt) => {
                                    if let OscPacket::Message(msg) = &pkt.1 {
                                        if pf_wl.contains(&msg.addr) {
                                            // Here sending the decoded packet's buffer instead of the UDP buffer
                                            // because some OSC libraries cant parse OSC packets with trailing NULL bytes.
                                            // However if malformed OSC packets are relayed due to PF allowing bad packets through they will be sent with a full MTU (NULL BYTES PADDED)
                                            let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                                            bcst_tx.send(encoded_packet_buf).unwrap();
                                            if let Some(ref dbgs) = debug_sender {
                                                routedbg::send_indbg_packet(
                                                    dbgs,
                                                    &buf,
                                                    Some(pkt.1.clone()),
                                                    address.to_string(),
                                                    routedbg::IncomingDebugMode::ALLOWED,
                                                );
                                            }
                                        } else {
                                            if let Some(ref dbgs) = debug_sender {
                                                routedbg::send_indbg_packet(
                                                    dbgs,
                                                    &buf,
                                                    Some(pkt.1.clone()),
                                                    address.to_string(),
                                                    routedbg::IncomingDebugMode::DROPPED,
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(_e) => {
                                    if !pf.filter_bad_packets {
                                        // Bad OSC packet routed
                                        bcst_tx.send(buf.to_vec()).unwrap();
                                        if let Some(ref dbgs) = debug_sender {
                                            routedbg::send_indbg_packet(
                                                dbgs,
                                                &buf,
                                                None,
                                                address.to_string(),
                                                routedbg::IncomingDebugMode::ALLOWED,
                                            );
                                        }
                                    } else {
                                        if let Some(ref dbgs) = debug_sender {
                                            routedbg::send_indbg_packet(
                                                dbgs,
                                                &buf,
                                                None,
                                                address.to_string(),
                                                routedbg::IncomingDebugMode::DROPPED,
                                            );
                                        }
                                    }
                                }
                            }
                        } else if pf.bl_enabled {
                            // Blacklist
                            match rosc::decoder::decode_udp(&buf) {
                                Ok(ref pkt) => {
                                    if let OscPacket::Message(msg) = &pkt.1 {
                                        if !pf_bl.contains(&msg.addr) {
                                            let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                                            bcst_tx.send(encoded_packet_buf).unwrap();

                                            if let Some(ref dbgs) = debug_sender {
                                                routedbg::send_indbg_packet(
                                                    dbgs,
                                                    &buf,
                                                    Some(pkt.1.clone()),
                                                    address.to_string(),
                                                    routedbg::IncomingDebugMode::ALLOWED,
                                                );
                                            }
                                        } else {
                                            if let Some(ref dbgs) = debug_sender {
                                                routedbg::send_indbg_packet(
                                                    dbgs,
                                                    &buf,
                                                    Some(pkt.1.clone()),
                                                    address.to_string(),
                                                    routedbg::IncomingDebugMode::DROPPED,
                                                );
                                            }
                                        }
                                    }
                                }
                                Err(_e) => {
                                    // Packet was bad should it still be sent?
                                    if !pf.filter_bad_packets {
                                        // Bad OSC packet routed
                                        bcst_tx.send(buf.to_vec()).unwrap();
                                        if let Some(ref dbgs) = debug_sender {
                                            routedbg::send_indbg_packet(
                                                dbgs,
                                                &buf,
                                                None,
                                                address.to_string(),
                                                routedbg::IncomingDebugMode::ALLOWED,
                                            );
                                        }
                                    } else {
                                        if let Some(ref dbgs) = debug_sender {
                                            routedbg::send_indbg_packet(
                                                dbgs,
                                                &buf,
                                                None,
                                                address.to_string(),
                                                routedbg::IncomingDebugMode::DROPPED,
                                            );
                                        }
                                    }
                                }
                            }
                        } else {
                            // No mode selected

                            if pf.filter_bad_packets {
                                // If filter bad packets enabled check if bad packet
                                if let Ok(pkt) = rosc::decoder::decode_udp(&buf) {

                                    let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                                    bcst_tx.send(encoded_packet_buf).unwrap();
                                    if let Some(ref dbgs) = debug_sender {
                                        routedbg::send_indbg_packet(
                                            dbgs,
                                            &buf,
                                            Some(pkt.1),
                                            address.to_string(),
                                            routedbg::IncomingDebugMode::ALLOWED,
                                        );
                                    }
                                } else {
                                    /*println!("[*] Filtered bad packet");*/
                                    if let Some(ref dbgs) = debug_sender {
                                        routedbg::send_indbg_packet(
                                            dbgs,
                                            &buf,
                                            None,
                                            address.to_string(),
                                            routedbg::IncomingDebugMode::DROPPED,
                                        );
                                    }
                                }
                            } else {
                                // If filter bad packets not enabled then send!
                                bcst_tx.send(buf.to_vec()).unwrap();
                                if let Some(ref dbgs) = debug_sender {
                                    // Try to get parsed packet
                                    if let Ok(pkt) = rosc::decoder::decode_udp(&buf) {
                                        routedbg::send_indbg_packet(
                                            dbgs,
                                            &buf,
                                            Some(pkt.1),
                                            address.to_string(),
                                            routedbg::IncomingDebugMode::ALLOWED,
                                        );
                                    } else {
                                        // Still ALLOWED because filter bad packets is disabled
                                        routedbg::send_indbg_packet(
                                            dbgs,
                                            &buf,
                                            None,
                                            address.to_string(),
                                            routedbg::IncomingDebugMode::ALLOWED,
                                        );
                                    }
                                }
                            }
                        }
                    } else {
                        // PF disabled
                        bcst_tx.send(buf.to_vec()).unwrap();
                        if let Some(ref dbgs) = debug_sender {
                            // Try to get parsed packet
                            if let Ok(pkt) = rosc::decoder::decode_udp(&buf) {
                                routedbg::send_indbg_packet(
                                    dbgs,
                                    &buf,
                                    Some(pkt.1),
                                    address.to_string(),
                                    routedbg::IncomingDebugMode::ALLOWED,
                                );
                            } else {
                                // Still ALLOWED because PF is disabled
                                routedbg::send_indbg_packet(
                                    dbgs,
                                    &buf,
                                    None,
                                    address.to_string(),
                                    routedbg::IncomingDebugMode::ALLOWED,
                                );
                            }
                        }
                    }

                    match router_rx.try_recv() {
                        Ok(sig) => {
                            if sig {
                                //println!("[!] VRC OSC thread shutdown");
                                return;
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
            Err(_e) => {
                ////println!("UDPSOCKERR: {}", _e);
                match router_rx.try_recv() {
                    Ok(sig) => {
                        if sig {
                            //println!("[!] VRC OSC thread shutdown");
                            return;
                        }
                    }
                    Err(_e) => {} ////println!("router_rx vrc recv fn : {}", _e);},
                }
            }
        } // vrc recv sock
    } // loop
}

pub fn route_main(
    router_bind_target: String,
    router_rx: Receiver<RouterMsg>,
    app_stat_tx: Sender<VORAppIdentifier>,
    configs: Vec<(VORConfig, i64)>,
    pf: PacketFilter,
    vor_queue_size: usize,
    async_mode: bool,
    debug_route_channels: Option<Sender<routedbg::DebugPacket>>,
    debug_config: Option<routedbg::VORDebugOptions>,
) {
    // Bind UDP listening socket
    let vrc_sock = match UdpSocket::bind(router_bind_target) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx.send(app_error(-1, -1, "Failed to bind VOR socket.".to_string()));
            return;
        }
    };

    // Setting this socket to timed blocking does not have a dramatic effect on message passing delays due to socket blocking
    vrc_sock.set_nonblocking(false).unwrap();
    let _ = vrc_sock.set_read_timeout(Some(std::time::Duration::from_secs(1)));

    // App route threads
    let mut artc = Vec::new();

    /*
        Create async runtime
    */
    let mut async_rt: Option<tokio::runtime::Runtime> = None;
    let mut async_threads = vec![];
    if async_mode {
        async_rt = Some(tokio::runtime::Runtime::new().unwrap());
    }

    // Router message broadcast channels
    let (bcst_tx, _bcst_rx) = broadcast::channel(vor_queue_size);

    for (app, id) in configs {
        let (router_tx, router_rx) = mpsc::channel();
        artc.push(router_tx);

        // App status sender
        let app_stat_tx_at = app_stat_tx.clone();
        
        // Create new RX for broadcast channel
        let bcst_app_rx = bcst_tx.subscribe();

        let app_debug_sender_clone = debug_route_channels.clone();
        let app_debug_config_clone = debug_config.clone();
        /*
            Spawn app routers in the async runtime
        */
        if async_mode {
            async_threads.push(async_rt.as_ref().unwrap().spawn(route_app_async(
                bcst_app_rx,
                router_rx,
                app_stat_tx_at,
                id,
                app,
                app_debug_sender_clone,
                app_debug_config_clone,
            )));
        } else {
            thread::spawn(move || {
                route_app(
                    bcst_app_rx,
                    router_rx,
                    app_stat_tx_at,
                    id,
                    app,
                    app_debug_sender_clone,
                    app_debug_config_clone,
                )
            });
        }
    }
    drop(_bcst_rx); // Dont need this rx

    let (osc_parse_tx, osc_parse_rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();

    thread::spawn(move || {
        parse_vrc_osc(
            bcst_tx,
            osc_parse_rx,
            pf,
            vrc_sock,
            debug_config,
            debug_route_channels,
        );
    });
    //println!("[+] Started VRChat OSC Router.");

    // Listen for GUI events
    // Add dynamic route handling here probably
    /*
        Handle Removing/Adding/Modifying Routes
    */
    loop {
        match router_rx.recv().unwrap() {
            RouterMsg::ShutdownAll => {
                // Send shutdown to all threads

                // Shutdown osc parse thread first
                osc_parse_tx.send(true).unwrap();

                //drop(vrc_sock);
                //println!("[*] Shutdown signal: OSC receive thread");

                // Shutdown app route threads
                for app_route_thread_channel in artc {
                    let _ = app_route_thread_channel.send(true);
                }
                //println!("[*] Shutdown signal: Route threads");

                if async_mode {
                    //println!("[*] Async runtime background shutdown.");

                    for h in async_threads {
                        h.abort();
                        async_rt.as_ref().unwrap().spawn(async {
                            let _ = h.await;
                        });
                    }
                    async_rt.unwrap().shutdown_background();
                }

                // Shutdown router thread last
                //println!("[*] Shutdown signal: Router thread");
                return; // Shutdown router thread.
            } //_ =>{},
        }
    }
}
