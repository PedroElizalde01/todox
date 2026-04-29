use anyhow::Result;
use ratatui::widgets::ListState;
use std::path::Path;

use crate::{model::Ticket, repository::load_dir};

pub type TicketListState = (Vec<Ticket>, ListState, String);

pub struct App {
    pub stack: Vec<TicketListState>,
    pub detail: bool,
}

impl App {
    pub fn new(root: Vec<Ticket>) -> Self {
        let select_first = !root.is_empty();
        Self {
            stack: vec![(root, list_state(select_first), "todo".into())],
            detail: false,
        }
    }

    pub fn cur(&self) -> &TicketListState {
        self.stack.last().expect("app stack always has root")
    }

    pub fn cur_mut(&mut self) -> &mut TicketListState {
        self.stack.last_mut().expect("app stack always has root")
    }

    pub fn selected(&self) -> Option<&Ticket> {
        let (tickets, state, _) = self.cur();
        state.selected().and_then(|index| tickets.get(index))
    }

    pub fn move_sel(&mut self, delta: i32) {
        let (tickets, state, _) = self.cur_mut();
        if tickets.is_empty() {
            return;
        }

        let current = state.selected().unwrap_or(0);
        let next = if delta >= 0 {
            (current + delta as usize) % tickets.len()
        } else {
            current
                .checked_sub(delta.unsigned_abs() as usize % tickets.len())
                .unwrap_or_else(|| {
                    tickets.len() - (delta.unsigned_abs() as usize - current) % tickets.len()
                })
                % tickets.len()
        };
        state.select(Some(next));
    }

    pub fn enter(&mut self) {
        if self.detail {
            if let Some(ticket) = self.selected().filter(|ticket| ticket.has_children()) {
                self.stack.push((
                    ticket.children.clone(),
                    list_state(true),
                    ticket.title.clone(),
                ));
                self.detail = false;
            }
            return;
        }

        if self.selected().is_some() {
            self.detail = true;
        }
    }

    pub fn back(&mut self) {
        if self.detail {
            self.detail = false;
            return;
        }

        if self.stack.len() > 1 {
            self.stack.pop();
        }
    }

    pub fn reload(&mut self, root: &Path) -> Result<()> {
        let snapshot = self.snapshot();
        let detail = self.detail;
        let tickets = load_dir(root)?;

        let select_first = !tickets.is_empty();
        self.stack.clear();
        self.stack
            .push((tickets, list_state(select_first), "todo".into()));

        for (depth, title) in snapshot.iter().enumerate() {
            let next = {
                let (tickets, state, _) = self.cur_mut();
                let Some(index) = tickets.iter().position(|ticket| ticket.title == *title) else {
                    break;
                };
                state.select(Some(index));

                if depth + 1 < snapshot.len() {
                    let child = &tickets[index];
                    if !child.has_children() {
                        None
                    } else {
                        Some((child.children.clone(), child.title.clone()))
                    }
                } else {
                    None
                }
            };

            let Some((children, label)) = next else {
                continue;
            };
            let select_first = !children.is_empty();
            self.stack.push((children, list_state(select_first), label));
        }

        self.detail = detail;
        Ok(())
    }

    fn snapshot(&self) -> Vec<String> {
        self.stack
            .iter()
            .filter_map(|(tickets, state, _)| state.selected().and_then(|index| tickets.get(index)))
            .map(|ticket| ticket.title.clone())
            .collect()
    }
}

fn list_state(select_first: bool) -> ListState {
    let mut state = ListState::default();
    if select_first {
        state.select(Some(0));
    }
    state
}
