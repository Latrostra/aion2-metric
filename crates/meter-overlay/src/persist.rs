use std::fs;
use std::io;
use std::path::Path;
use std::sync::mpsc::{Sender, channel};

pub fn write_atomically(dir: &Path, file_name: &str, contents: &[u8]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{file_name}.tmp"));
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, dir.join(file_name))
}

pub fn spawn_latest_value_saver<T: Send + 'static>(
    save: impl Fn(&T) + Send + 'static,
) -> Sender<T> {
    let (sender, receiver) = channel::<T>();
    std::thread::spawn(move || {
        while let Ok(mut latest) = receiver.recv() {
            while let Ok(newer) = receiver.try_recv() {
                latest = newer;
            }
            save(&latest);
        }
    });
    sender
}
