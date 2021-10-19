use chat::constants::{ADDR, PORT};
use chat::message::Message;
#[allow(unused_imports)]
use log::{debug, error, info, warn};
use std::io;
use tokio::io::{split, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::{join, select};

async fn send_msg_to_client(msg: &Message, client: &mut WriteHalf<TcpStream>) {
    debug!("Sending {:?}", msg);
    let mut msg_buf = Vec::<u8>::new();
    msg.write_out(&mut msg_buf);
    let _ = client.write_all(&mut msg_buf).await;
}

async fn client_writer(
    mut msg_rx: Receiver<Message>,
    mut client_rx: Receiver<WriteHalf<TcpStream>>,
) {
    let mut all_clients = Vec::<WriteHalf<TcpStream>>::new();
    let mut all_messages = Vec::<Message>::new();

    loop {
        select! {
            Some(new_msg) = msg_rx.recv() => {
                debug!("Sending {:?} to all clients.", new_msg);
                for client in all_clients.iter_mut() {
                    // TODO: concurrent
                    send_msg_to_client(&new_msg, client).await;
                }
                all_messages.push(new_msg);
            }
            new_client = client_rx.recv() => {
                if let Some(new_client) = new_client {
                    debug!("Adding new client to listeners.");
                    // for msg in all_messages.iter() {
                    //     send_msg_to_client(msg, &mut new_client).await;
                    // }
                    all_clients.push(new_client);
                }
            }
        }
    }
}

async fn handle_client(
    mut receive_half: ReadHalf<TcpStream>,
    msg_tx: Sender<Message>,
) -> io::Result<()> {
    loop {
        let mut buf = [0u8; 128];
        let n = receive_half.read(&mut buf[..]).await?;

        debug!("Got {} bytes from the client.", n);

        if n == 0 {
            info!("Connection terminated.");
            break;
        }

        let msg = Message::read_in(&buf[..]);
        msg_tx.send(msg).await.unwrap();
    }
    Ok(())
}

async fn main_loop(listener: TcpListener) -> io::Result<()> {
    let (msgs_tx, msgs_rx) = channel(100);
    let (client_writers_tx, client_writers_rx) = channel(100);

    tokio::spawn(async move {
        client_writer(msgs_rx, client_writers_rx).await;
    });

    loop {
        let (socket, addr) = listener.accept().await?;
        let (receive_half, send_half) = split(socket);
        client_writers_tx.clone().send(send_half).await.unwrap();

        info!("Accepted connection from {:?}", addr);

        let msgs_tx_clone = msgs_tx.clone();
        tokio::spawn(async move {
            info!("Spawned new client task.");
            let _ = handle_client(receive_half, msgs_tx_clone).await;
        });
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    info!("Binding to {:?}", format!("{}:{}", ADDR, PORT));
    let listener = TcpListener::bind(format!("{}:{}", ADDR, PORT)).await?;

    let server_handle = tokio::spawn(async move {
        main_loop(listener).await.unwrap();
    });

    join!(server_handle).0.unwrap();

    Ok(())
}
