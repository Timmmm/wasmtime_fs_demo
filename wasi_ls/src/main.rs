use std::fs;
use std::io;

fn main() -> io::Result<()> {
    let entries = fs::read_dir(".")?;

    for entry_result in entries {
        match entry_result {
            Ok(entry) => println!("{}", entry.file_name().to_string_lossy()),
            Err(e) => eprintln!("Error reading directory entry: {}", e),
        }
    }

    Ok(())
}
