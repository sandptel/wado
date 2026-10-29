// SPDX-License-Identifier: AGPL-3.0-only
//! Keeping a `VecModel` in step with fresh rows by changing only what changed. A row that did
//! not change is not touched, so its repeater instance keeps its state (a focused field, a
//! running animation) and nothing is re-laid-out for nothing.

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

pub fn sync<T: Clone + PartialEq + 'static>(model: &Rc<VecModel<T>>, rows: Vec<T>) {
    let old = model.row_count();
    for (i, r) in rows.iter().enumerate() {
        if i < old {
            if model.row_data(i).as_ref() != Some(r) {
                model.set_row_data(i, r.clone());
            }
        } else {
            model.push(r.clone());
        }
    }
    for i in (rows.len()..old).rev() {
        model.remove(i);
    }
}

pub fn rc<T: Clone + 'static>(m: &Rc<VecModel<T>>) -> ModelRc<T> {
    ModelRc::from(m.clone())
}

/// A plain list for a field of a struct row (`[string]` in Slint).
pub fn strings<I: IntoIterator<Item = S>, S: Into<slint::SharedString>>(
    it: I,
) -> ModelRc<slint::SharedString> {
    ModelRc::new(VecModel::from(
        it.into_iter()
            .map(Into::into)
            .collect::<Vec<slint::SharedString>>(),
    ))
}
