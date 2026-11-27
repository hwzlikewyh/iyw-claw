use super::*;

#[test]
#[cfg(unix)]
fn unix_git_archive_accepts_current_directory_prefix() {
    let root = std::env::temp_dir().join(format!("iyw-git-archive-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let mut builder = tar::Builder::new(Vec::new());
    let mut directory = tar::Header::new_gnu();
    directory.set_entry_type(EntryType::Directory);
    directory.set_size(0);
    directory.set_mode(0o755);
    directory.set_cksum();
    builder.append_data(&mut directory, "./", io::empty()).unwrap();
    let contents = b"git executable fixture";
    let mut file = tar::Header::new_gnu();
    file.set_size(contents.len() as u64);
    file.set_mode(0o755);
    file.set_cksum();
    builder.append_data(&mut file, "./bin/git", &contents[..]).unwrap();
    let bytes = builder.into_inner().unwrap();
    unpack_tar(&bytes[..], &root).unwrap();
    assert_eq!(fs::read(root.join("bin/git")).unwrap(), contents);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn archive_rejects_parent_and_absolute_paths() {
    for value in ["../git", "./../git", "bin/../../git", "/bin/git", "C:/git"] {
        assert!(reject_unsafe_path(Path::new(value)).is_err(), "accepted {value}");
    }
}
