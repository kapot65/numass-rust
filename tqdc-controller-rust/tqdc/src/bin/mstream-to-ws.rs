// TODO move to server crate
// use tokio::net::UdpSocket;
// use tokio::sync::watch::channel;

// use std::{io, net::SocketAddrV4};

// use futures::SinkExt;
// use futures_util::StreamExt;
// use tokio::net::TcpListener;

// use common::{WS, FrameChannel};
// use tqdc::mlink::MlinkMessage;

// #[tokio::main]
// async fn main() -> io::Result<()> {

//     let (tx, rx) = channel::<Option<Vec<u8>>>(None);

//     tokio::spawn(async move  {
//         let addr = "127.0.0.1:8082".parse::<SocketAddrV4>().unwrap();
//         let listener = TcpListener::bind(addr).await.unwrap();
//         println!("Listening on: {}", addr);

//         while let Ok((stream, add)) = listener.accept().await {

//             println!("{:?}", add);
//             let mut rx = rx.clone();

//             tokio::spawn(async move {

//                 let ws_stream = tokio_tungstenite::accept_async(stream).await.unwrap();

//                 let (mut write, _) = ws_stream.split();

//                 while rx.changed().await.is_ok() {
//                     let msg =  rx.borrow().clone().unwrap();
//                     // TODO: handle websocket disconnects
//                     write.send(tokio_tungstenite::tungstenite::Message::Binary(msg)).await.unwrap();
//                 }
//             });
//         }
//     });

//     // communication with MStream
//     {
//         let sock = UdpSocket::bind("0.0.0.0:33301").await?;
//         let to_addr = "10.0.0.5:33301".parse::<SocketAddrV4>().unwrap();

//         sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
//             0x0000, 
//             0x0101, 
//             0x0001, 
//             0xFFFF, 
//             0xFFFF
//         )), to_addr).await?;


//         loop {
//             let mut buf = [0; 4096 * 10];
//             let (len, _) = sock.recv_from(&mut buf).await?;

//             let message = MlinkMessage::from_datagram(&buf[..len]);

//             println!("{:?}", &message);

//             match message {
//                 MlinkMessage::StreamReq { header, frames } => {
//                     if let Some(frame) = frames.first() {

//                         sock.send_to(&MlinkMessage::to_datagram(&MlinkMessage::new_stream_acq(
//                             header.seq, 
//                             0x0101, 
//                             0x0001, 
//                             frame.fragment_offset, 
//                             frame.fragment_id, 
//                         )), to_addr).await?;

//                         let channels = frames.last().unwrap().channels.iter().map(|ch| {
//                             FrameChannel {
//                                 id: ch.id,
//                                 bins: ch.bins.clone()
//                             }
//                         }).collect::<Vec<_>>();

//                         let buf = bendy::serde::to_bytes(&WS::GraphFrame(channels)).unwrap();
//                         tx.send(Some(buf)).unwrap();
//                     } else {
//                         panic!("no frames in package!")
//                     }
//                 }
//                 _ => panic!("unimplemented!!")
//             }
//         }
//     }
// }