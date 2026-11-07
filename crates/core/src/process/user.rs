//! The user this process runs as, which is who may reach what it serves.

/// This process's real user id.
pub fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;

    /// A file this process creates belongs to the user it runs as, which the
    /// file system says apart from this function.
    #[test]
    fn the_uid_is_the_owner_of_what_this_process_creates() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mine");
        std::fs::write(&file, "mine").unwrap();

        assert_eq!(super::uid(), std::fs::metadata(&file).unwrap().uid());
    }
}
