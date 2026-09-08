use super::{
    MarkerlessDirectoryState, ensure_bucket, ensure_entry, ensure_project,
    inspect_markerless_directory,
};
use std::{fs, os::unix::fs::symlink};

#[test]
fn project_and_entry_symlinks_never_mutate_their_targets() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    let dependency = case.path().join("node_modules/package");
    fs::create_dir_all(&cache).unwrap();
    fs::create_dir_all(&dependency).unwrap();
    let sentinel = dependency.join("sentinel.txt");
    fs::write(&sentinel, "owned by dependency\n").unwrap();

    let bucket = ensure_bucket(&cache, "00").unwrap();
    let project_digest = format!("{:064x}", 1);
    symlink(&dependency, bucket.join(&project_digest)).unwrap();
    assert!(ensure_project(&bucket, &project_digest).is_err());
    assert_eq!(
        fs::read_to_string(&sentinel).unwrap(),
        "owned by dependency\n"
    );

    fs::remove_file(bucket.join(&project_digest)).unwrap();
    let project = ensure_project(&bucket, &project_digest).unwrap();
    let entry_digest = format!("{:064x}", 2);
    symlink(&dependency, project.join(&entry_digest)).unwrap();
    assert!(ensure_entry(&project, &entry_digest).is_err());
    assert_eq!(
        fs::read_to_string(&sentinel).unwrap(),
        "owned by dependency\n"
    );
}

#[test]
fn a_foreign_digest_directory_is_never_adopted() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let bucket = ensure_bucket(&cache, "00").unwrap();
    let digest = format!("{:064x}", 3);
    let foreign = bucket.join(&digest);
    fs::create_dir(&foreign).unwrap();
    let sentinel = foreign.join("foreign.txt");
    fs::write(&sentinel, "foreign\n").unwrap();

    assert!(ensure_project(&bucket, &digest).is_err());
    assert_eq!(fs::read_to_string(sentinel).unwrap(), "foreign\n");
}

#[test]
fn an_in_progress_bootstrap_lock_is_the_only_adoptable_regular_file() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let bucket = cache.join("ab");
    fs::create_dir(&bucket).unwrap();
    fs::write(bucket.join(".publish.lock"), []).unwrap();

    assert_eq!(ensure_bucket(&cache, "ab").unwrap(), bucket);
    assert_eq!(
        fs::read_to_string(bucket.join(".bucket-owner")).unwrap(),
        "vize-nuxt-bucket:v2:ab\n"
    );
}

#[test]
fn a_pending_named_file_without_the_bootstrap_lock_is_foreign() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let digest = format!("{:064x}", 4);
    let foreign = cache.join(&digest);
    fs::create_dir(&foreign).unwrap();
    let pending = foreign.join(".vize-nuxt-config-1-foreign.pending");
    fs::write(&pending, "foreign\n").unwrap();

    assert!(ensure_project(&cache, &digest).is_err());
    assert_eq!(fs::read_to_string(pending).unwrap(), "foreign\n");
    assert!(!foreign.join(".project-owner").exists());
}

#[test]
fn a_marker_published_during_markerless_inspection_is_not_foreign() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let bucket = cache.join("ef");
    fs::create_dir(&bucket).unwrap();
    fs::write(bucket.join(".bucket-owner"), "vize-nuxt-bucket:v2:ef\n").unwrap();

    assert!(matches!(
        inspect_markerless_directory(&bucket, ".bucket-owner").unwrap(),
        MarkerlessDirectoryState::Published
    ));
    assert_eq!(ensure_bucket(&cache, "ef").unwrap(), bucket);
}

#[test]
fn concurrent_first_users_publish_one_exact_bucket_identity() {
    let case = tempfile::tempdir().unwrap();
    let cache = case.path().join("cache");
    fs::create_dir(&cache).unwrap();
    let start = std::sync::Arc::new(std::sync::Barrier::new(3));
    let tasks = (0..2)
        .map(|_| {
            let cache = cache.clone();
            let start = std::sync::Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                ensure_bucket(&cache, "cd").unwrap()
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    let paths = tasks
        .into_iter()
        .map(|task| task.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(paths[0], paths[1]);
    assert_eq!(
        fs::read_to_string(paths[0].join(".bucket-owner")).unwrap(),
        "vize-nuxt-bucket:v2:cd\n"
    );
}
