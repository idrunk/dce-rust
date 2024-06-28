use std::net::SocketAddr;
use futures::StreamExt;
use futures_util::{future, SinkExt};
use tokio::io;
use tokio::net::{TcpStream, UdpSocket};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::bytes::Bytes;
use tokio_util::codec::{BytesCodec, Decoder, FramedRead, FramedWrite};
use tokio_util::udp::UdpFramed;
use url::Url;
use dce_cli::protocol::{CliProtocol, CliRaw};
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_util::mixed::DceErr;
use rand::random;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use dce_macro::{api, closed_err};

pub fn append(router: Router<CliProtocol>) -> Router<CliProtocol> {
    router.push(tcp_interactive)
        .push(udp_interactive)
        .push(websocket_interactive)
        .push(tcp)
        .push(udp)
        .push(websocket)
}


/// `cargo run --bin app --target-dir target/tcp-interactive -- tcp interactive 127.0.0.1:2048`
/// and then type in:
/// `hello>BODY>>>`
#[api("tcp/interactive/{address}")]
pub async fn tcp_interactive(req: CliRaw) {
    let mut stdin = FramedRead::new(io::stdin(), BytesCodec::new())
        .map(|i| i.map(|bytes| bytes));
    let mut stdout = FramedWrite::new(io::stdout(), BytesCodec::new());
    let addr = req.param("address")?.as_str().unwrap().parse::<SocketAddr>().expect("not a valid socket address");

    let stream = TcpStream::connect(addr).await.expect("tcp connect failed");
    let (mut sink, mut stream) = BytesCodec::new().framed(stream).split();

    match future::join(sink.send_all(&mut stdin), stdout.send_all(&mut stream)).await {
        (Err(e), _) | (_, Err(e)) => DceErr::closed0_wrap(e),
        _ => Ok(None),
    }
}

/// `cargo run --bin app --target-dir target/udp-interactive -- udp interactive 127.0.0.1:2049`
/// and then type in:
/// `hello>BODY>>>`
#[api("udp/interactive/{address}")]
pub async fn udp_interactive(req: CliRaw) {
    let mut stdin = FramedRead::new(io::stdin(), BytesCodec::new())
        .map(|i| i.map(|bytes| bytes));
    let mut stdout = FramedWrite::new(io::stdout(), BytesCodec::new());
    let addr = req.param("address")?.as_str().unwrap().parse::<SocketAddr>().expect("not a valid socket address");

    let socket = UdpSocket::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).await.expect("udp bind failed");
    socket.connect(&addr).await.expect("failed to connect to the remote udp");
    let (mut sink, mut stream) = UdpFramed::new(socket, BytesCodec::new()).split();

    match future::join(
        tokio::spawn(async move {
            while let Some(Ok(input)) = stdin.next().await {
                sink.send((input, addr)).await.expect("failed to send");
            }
        }),
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok((msg, _)) => stdout.send(msg).await.expect("failed write"),
                    Err(e) => println!("failed to read from socket; error={}", e),
                };
            }
        })
    ).await {
        (Err(e), _) | (_, Err(e)) => DceErr::closed0_wrap(e),
        _ => Ok(None)
    }
}

/// `cargo run --bin app --target-dir target/websocket-interactive -- websocket interactive 127.0.0.1:2047`
/// and then type in:
/// `hello>BODY>>>`
#[api("websocket/interactive/{address}")]
pub async fn websocket_interactive(req: CliRaw) {
    let mut stdin = FramedRead::new(io::stdin(), BytesCodec::new())
        .map(|i| i.map(|bytes| bytes));
    let mut stdout = FramedWrite::new(io::stdout(), BytesCodec::new());
    let url = Url::parse(&format!("ws://{}/", req.param("address")?.as_str().unwrap())).unwrap();

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    let (mut sink, mut stream) = ws_stream.split();
    match future::join(
        tokio::spawn(async move {
            while let Some(Ok(input)) = stdin.next().await {
                sink.send(Message::Binary(input.to_vec())).await.expect("failed to send");
            }
        }),
        tokio::spawn(async move {
            while let Some(msg) = stream.next().await {
                match msg {
                    Ok(msg) => stdout.send(Bytes::from(msg.into_data())).await.expect("failed write"),
                    Err(e) => println!("failed to read from socket; error={}", e),
                };
            }
        })
    ).await {
        (Err(e), _) | (_, Err(e)) => DceErr::closed0_wrap(e),
        _ => Ok(None)
    }
}


/// `cargo run --bin app -- tcp 127.0.0.1:2048 -- hello`
/// `cargo run --bin app -- tcp 127.0.0.1:2048 -- echo "echo me"`
#[api("tcp/{address}")]
pub async fn tcp(req: CliRaw) {
    let addr = req.param("address")?.as_str().unwrap().parse::<SocketAddr>().expect("not a valid socket address");
    let stream = TcpStream::connect(addr).await.expect("tcp connect failed");
    let (mut sink, mut stream) = BytesCodec::new().framed(stream).split();

    let pass = req.rp().pass();
    assert!(! pass.is_empty(), "pass args cannot be empty");
    match sink.send(Bytes::from(format!("0;{}>BODY>>>{}", pass.join("/"), random::<usize>()))).await {
        Ok(_) => match stream.next().await {
            Some(Ok(msg)) => req.pack(Serialized::Bytes(msg.freeze())),
            _ => Err(closed_err!("failed to receive message")),
        },
        Err(err) => DceErr::closed0_wrap(err),
    }
}

/// `cargo run --bin app -- udp 127.0.0.1:2049 -- hello`
/// `cargo run --bin app -- udp 127.0.0.1:2049 -- echo "echo me"`
#[api("udp/{address}")]
pub async fn udp(req: CliRaw) {
    let addr = req.param("address")?.as_str().unwrap().parse::<SocketAddr>().expect("not a valid socket address");
    let socket = UdpSocket::bind("0.0.0.0:0".parse::<SocketAddr>().unwrap()).await.expect("udp connect failed");
    socket.connect(&addr).await.unwrap();
    let (mut sink, mut stream) = UdpFramed::new(socket, BytesCodec::new()).split();

    let pass = req.rp().pass();
    assert!(! pass.is_empty(), "pass args cannot be empty");
    match sink.send((Bytes::from(format!("0;{}>BODY>>>{}", pass.join("/"), random::<usize>())), addr)).await {
        Ok(_) => match stream.next().await {
            Some(Ok((msg, _))) => req.pack(Serialized::Bytes(msg.freeze())),
            _ => Err(closed_err!("failed to receive message")),
        },
        Err(err) => DceErr::closed0_wrap(err),
    }
}

/// `cargo run --bin app -- websocket 127.0.0.1:2047 -- hello`
/// `cargo run --bin app -- websocket 127.0.0.1:2047 -- echo "echo me"`
#[api("websocket/{address}")]
pub async fn websocket(req: CliRaw) {
    let addr = req.param("address")?.as_str().unwrap();    
    let url = Url::parse(&format!("ws://{}/", addr)).unwrap();
    let mut ws_req = url.into_client_request().unwrap();
    if let Some(sid) = req.rp().args().get("--sid") {
        ws_req.headers_mut().insert("X-Session-Id", HeaderValue::from_str(sid).unwrap());
    }
    let data = req.rp().args().get("--data").map_or_else(|| random::<usize>().to_string(), Clone::clone);
    let (ws_stream, _) = connect_async(ws_req).await.expect("Failed to connect");
    let (mut sink, mut stream) = ws_stream.split();

    let pass = req.rp().pass();
    assert!(! pass.is_empty(), "pass args cannot be empty");
    match sink.send(Message::from(format!("0;{}>BODY>>>{}", pass.join("/"), data))).await {
        Ok(_) => match stream.next().await {
            Some(Ok(msg)) => req.pack(Serialized::String(msg.to_string())),
            _ => Err(closed_err!("failed to receive message")),
        },
        Err(err) => DceErr::closed0_wrap(err),
    }
}
