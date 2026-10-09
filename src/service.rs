//! Single-user local socket service using a bounded, verified FlatBuffers transport.
use crate::{
    config::Config,
    engine::{Engine, Outcome},
    wire::{self, Message},
};
use anyhow::{bail, Result};
use std::{
    os::unix::fs::{FileTypeExt, PermissionsExt},
    sync::Arc,
};
use tokio::{
    io::AsyncWriteExt,
    net::{UnixListener, UnixStream},
};

pub async fn serve(engine: Engine) -> Result<()> {
    let socket = engine.cfg.socket();
    if let Ok(meta) = std::fs::symlink_metadata(&socket) {
        if !meta.file_type().is_socket() {
            bail!("Configured socket path is not a socket")
        }
        bail!("Socket exists; refusing to delete a possible active daemon socket");
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let engine = Arc::new(engine);
    let capacity = Arc::new(tokio::sync::Semaphore::new(1));
    println!("Listening on {}", socket.display());
    loop {
        tokio::select! {
            stream=listener.accept()=>{
                let (mut stream,_)=stream?;
                let Ok(permit)=capacity.clone().try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let engine=engine.clone();
                tokio::spawn(async move {
                    let _permit=permit;
                    let response=match tokio::time::timeout(std::time::Duration::from_secs(10),wire::read_message(&mut stream)).await {
                        Ok(Ok(Message::Ask{id,query,mode}))=>{
                            match engine.run(&query,&mode).await {
                                Ok(outcome)=>Message::Reply{id,outcome},
                                Err(err)=>Message::Error{id,message:err.to_string()},
                            }
                        }
                        Ok(Ok(_))=>Message::Error{id:"invalid".into(),message:"Expected Ask message".into()},
                        Ok(Err(err))=>Message::Error{id:"invalid".into(),message:err.to_string()},
                        Err(_)=>Message::Error{id:"timeout".into(),message:"Request timed out".into()},
                    };
                    let _=wire::write_message(&mut stream,&response).await;
                    let _=stream.shutdown().await;
                });
            }
            _=tokio::signal::ctrl_c()=>{break}
        }
    }
    std::fs::remove_file(socket)?;
    Ok(())
}

pub async fn request(cfg: &Config, query: String, mode: String) -> Result<Outcome> {
    let mut stream = UnixStream::connect(cfg.socket()).await?;
    let message = wire::new_request(query, mode);
    let id = match &message {
        Message::Ask { id, .. } => id.clone(),
        _ => unreachable!(),
    };
    wire::write_message(&mut stream, &message).await?;
    match tokio::time::timeout(
        std::time::Duration::from_secs(cfg.max_wall_seconds + 10),
        wire::read_message(&mut stream),
    )
    .await??
    {
        Message::Reply {
            id: answer_id,
            outcome,
        } if answer_id == id => Ok(outcome),
        Message::Error { message, .. } => bail!("Server error: {message}"),
        _ => bail!("Unexpected or mismatched server reply"),
    }
}
