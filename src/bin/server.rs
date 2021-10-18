use chat::message::Message;
#[allow(unused_imports)]
use log::{debug, error, info, warn};
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::join;
use tokio::net::{TcpListener, TcpStream};

async fn handle_client(mut socket: TcpStream) -> io::Result<()> {
    loop {
        let mut buf = [0u8; 128];
        let n = socket.read(&mut buf[..]).await?;

        if n == 0 {
            info!("Connection terminated.");
            break;
        }

        let msg = Message::read_in(&buf[..]);

        debug!("Got {:?} from client: {:?}", msg, buf);
    }
    Ok(())
}

async fn main_loop(listener: TcpListener) -> io::Result<()> {
    loop {
        let (socket, addr) = listener.accept().await?;
        info!("Accepted connection from {:?}", addr);

        tokio::spawn(async move {
            let _ = handle_client(socket).await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let addr = "127.0.0.1:8080";
    info!("Binding to {:?}", addr);
    let listener = TcpListener::bind(addr).await?;

    let server_handle = tokio::spawn(async move {
        main_loop(listener).await.unwrap();
    });

    let msg = Message {
        author: "Riley".to_owned(),
        content: "Hello, there.".to_owned(),
    };

    let mut buf = Vec::<u8>::new();
    msg.write_out(&mut buf);

    let mut stream = TcpStream::connect(addr).await?;

    stream.write_all(&mut buf).await?;

    debug!("{:?} serialized is {:?}", msg, &mut buf);

    join!(server_handle).0.unwrap();

    Ok(())
}
