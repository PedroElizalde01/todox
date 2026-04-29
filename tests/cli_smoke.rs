use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use todox::repository::{find_root, load_dir};

fn temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("todox-it-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn loads_from_plain_todo_directory() {
    let root = temp_dir();
    let todo = root.join("todo");
    fs::create_dir_all(&todo).expect("create todo dir");
    fs::write(todo.join("task.toon"), "title: Task").expect("write ticket");

    let found = find_root(&root).expect("find todo root");
    let tickets = load_dir(&found).expect("load tickets");

    assert_eq!(found, todo);
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].title, "Task");

    let _ = fs::remove_dir_all(root);
}
