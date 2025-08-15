use std::fs;
use std::path::Path;

fn main() {
    print_tree(Path::new("."), &mut Vec::new());
}

fn print_tree(path: &Path, is_last_child: &mut Vec<bool>) {
    for (i, last) in is_last_child.iter().enumerate() {
        print!(
            "{}",
            match (i + 1 == is_last_child.len(), *last) {
                (true, true) => "└── ",
                (true, false) => "├── ",
                (false, true) => "    ",
                (false, false) => "│   ",
            }
        );
    }

    if let Some(name) = path.file_name() {
        print!("{}", name.to_string_lossy());
    } else {
        print!("{}", path.display());
    }

    if !path.is_dir()
        && let Ok(format) = file_format::FileFormat::from_file(path)
        && format != file_format::FileFormat::ArbitraryBinaryData
    {
        println!(" - {}", format.name());
    } else {
        println!();
    }

    if path.is_dir() {
        if let Ok(read_dir) = fs::read_dir(path) {
            let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.path());

            let len = entries.len();
            for (i, entry) in entries.into_iter().enumerate() {
                is_last_child.push(i == len - 1);
                print_tree(&entry.path(), is_last_child);
                is_last_child.pop();
            }
        }
    }
}
