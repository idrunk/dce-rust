use futures_util::StreamExt;
use log::info;
use tokio::net::UdpSocket;
use tokio_util::codec::BytesCodec;
use tokio_util::udp::UdpFramed;
use dce_cli::protocol::CliRaw;
use dce_macro::api;
use dce_router::protocol::RoutableProtocol;
use dce_router::router::Router;
use dce_router::serializer::Serialized;
use dce_tokio::protocol::{SemiTcpProtocol, SemiTcpRaw};

/// `cargo run --package dce --bin app -- udp start`
#[api("udp/start")]
pub async fn udp_start(req: CliRaw) {
    let addr = "0.0.0.0:2049";
    let socket = UdpSocket::bind(addr).await.expect(format!("failed bind udp on {}", addr).as_str());
    let router = Router::new()
        .push(hello)
        .push(echo)
        .ready();

    info!("Dce started at {} with tokio-udp", addr);

    let (mut sink, mut stream) = UdpFramed::new(socket, BytesCodec::new()).split();

    while let Some(msg) = stream.next().await {
        match msg {
            Ok((msg, addr)) => SemiTcpProtocol::from(msg).udp_route(router.clone(), &mut sink, addr, Default::default()).await,
            Err(err) => println!("Socket closed with error: {:?}", err)
        };
    }
    req.end(None)
}


/// `cargo run --package dce --bin app -- udp 127.0.0.1:2049 -- hello`
#[api]
pub async fn hello(req: SemiTcpRaw) {
    req.pack_resp(Serialized::String("hello world".to_string()))
}

/// `cargo run --package dce --bin app -- udp 127.0.0.1:2049 -- echo "echo me"`
#[api("echo/{param?}")]
pub async fn echo(mut req: SemiTcpRaw) {
    let body = req.rpi_mut().body().await?;
    let param = req.param("param")?.get().unwrap_or("");
    let body = format!(r#"path param data: "{}"{}body data: "{}""#, param, "\n", body);
    req.pack_resp(Serialized::String(body))
}
