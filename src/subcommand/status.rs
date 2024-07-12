use crate::index::{Index, IndexEntry};
use ansi_term::{Color, Style};
use eyre::{Context, Result};
use std::collections::HashMap;

pub fn run() -> Result<()> {
    // TODO: display current branch
    // TODO: compare branch to remote
    // TODO: compare HEAD to index

    let index = Index::read_default().context("read index")?;
    let mut working_tree: HashMap<String, IndexEntry> = {
        let Index { entries, .. } = Index::working_tree(".").context("read working tree")?;
        HashMap::from_iter(entries.into_iter().map(|entry| (entry.name.clone(), entry)))
    };

    let mut modified = Vec::new();
    let mut deleted = Vec::new();
    for entry in index.entries.iter() {
        match working_tree.get(&entry.name) {
            Some(working_copy) => {
                if entry.hash != working_copy.hash {
                    modified.push(entry.name.clone());
                }

                working_tree.remove(&entry.name);
            }
            None => {
                deleted.push(entry.name.clone());
            }
        }
    }

    let mut added = working_tree.keys().collect::<Vec<_>>();
    added.sort_unstable();

    // ---

    let head = std::fs::read_to_string(".git/HEAD").context("read .git/HEAD")?;
    if !head.starts_with("ref: ") {
        println!("In detached head mode, at {}\n", head);
    } else {
        assert!(
            head.starts_with("ref: refs/heads/"),
            "lazy assumption about branch naming"
        );
        println!("On branch {}", &head[16..]);
    }

    if !modified.is_empty() || !deleted.is_empty() {
        println!("Changes not staged for commit:");
        println!(
            "  {}",
            Style::new()
                .dimmed()
                .paint("(use \"git add <file>...\" to update what will be commmitted)")
        );
        println!(
            "  {}",
            Style::new()
                .dimmed()
                .paint("(use \"git restore <file>...\" to discard changes in working directory)")
        );

        for file in modified.iter() {
            println!(
                "\t{} {} {}",
                Style::new().dimmed().fg(Color::Yellow).paint("[~]"),
                Style::new().italic().fg(Color::Yellow).paint("modified:"),
                Style::new().bold().fg(Color::Yellow).paint(file),
            );
        }

        for file in deleted.iter() {
            println!(
                "\t{} {} {}",
                Style::new().dimmed().fg(Color::Red).paint("[x]"),
                Style::new().italic().fg(Color::Red).paint("deleted:"),
                Style::new().bold().fg(Color::Red).paint(file),
            );
        }

        println!();
    }

    if !added.is_empty() {
        println!("Untracked files:");
        println!(
            "  {}",
            Style::new()
                .dimmed()
                .paint("(use \"git add <file>...\" to include in what will be committed)")
        );

        for file in added {
            println!(
                "\t{} {} {}",
                Style::new().dimmed().fg(Color::Green).paint("[+]"),
                Style::new().italic().fg(Color::Green).paint("added:"),
                Style::new().bold().fg(Color::Green).paint(file),
            );
        }

        println!();
    }

    Ok(())
}
