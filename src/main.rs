#![feature(generators)]
#![feature(generator_trait)]
#![feature(iter_from_generator)]
#![feature(impl_trait_in_fn_trait_return)]

use std::sync::Mutex;

use lazy_static::lazy_static;
use log::Level;
use url::Url;
use wasm_bindgen::prelude::*;
use web_sys::*;

mod brain;
mod inputs;

use brain::*;
use inputs::*;

lazy_static! {
    pub static ref GRID: Mutex<GridState> = Mutex::new(GridState { cells: vec![] });
}

pub fn get_document() -> Document {
    window()
        .expect("Couldn't get the window")
        .document()
        .expect("Couldn't get the document")
}

fn name_from_number(n: usize) -> String {
    let i = n % 26;
    let ch = char::from_u32(65 + i as u32).unwrap();
    if n / 26 > 0 {
        format!("{}{ch}", name_from_number(n / 26 - 1))
    } else {
        ch.into()
    }
}

pub fn setup_cell(cell: &Element, x: usize, y: usize) -> Result<(), JsValue> {
    cell.set_text_content(None);
    cell.set_class_name("");

    let id = format!("{x}x{y}");
    cell.set_id(&id);

    let cell_click_closure = Closure::wrap(Box::new(move || {
        {
            let cells = &mut GRID.lock().expect("Attempt to use locked mutex").cells;

            let new_value = match cells[x][y] {
                CellState::EMPTY => CellState::MISS,
                CellState::MISS => CellState::HIT,
                CellState::HIT => {
                    let grid_size = get_inputs().grid_size;

                    let mut near_queue = vec![(x, y)];
                    while let Some((x, y)) = near_queue.pop() {
                        for (x, y) in
                            std::iter::from_generator(get_neumann_neighbors(grid_size, x, y))
                        {
                            if cells[x][y] == CellState::HIT {
                                cells[x][y] = CellState::SUNK;
                                near_queue.push((x, y))
                            }
                        }
                    }

                    CellState::SUNK
                }
                CellState::SUNK => {
                    let grid_size = get_inputs().grid_size;

                    let mut near_queue = vec![(x, y)];
                    while let Some((x, y)) = near_queue.pop() {
                        for (x, y) in
                            std::iter::from_generator(get_neumann_neighbors(grid_size, x, y))
                        {
                            if cells[x][y] == CellState::SUNK {
                                cells[x][y] = CellState::HIT;
                                near_queue.push((x, y))
                            }
                        }
                    }

                    CellState::EMPTY
                }
            };

            cells[x][y] = new_value;
        }

        refresh();
    }) as Box<dyn FnMut()>);

    cell.add_event_listener_with_callback("click", cell_click_closure.as_ref().unchecked_ref())?;
    cell_click_closure.forget();

    Ok(())
}

pub fn refresh() {
    let inputs = get_inputs();

    let chances = calculate_chances(
        &GRID.lock().expect("Attempt to use locked mutex").cells,
        inputs.grid_size,
        &inputs.ships,
    );

    display_chances(chances);
}

pub fn display_chances(chances: Vec<Vec<usize>>) {
    let document = get_document();
    let inputs = get_inputs();

    let max_cell_chance = {
        let cells = &GRID.lock().expect("Attempt to use locked mutex").cells;

        if chances.iter().all(|row| row.iter().all(|&c| c == 0)) {
            0
        } else {
            chances
                .iter()
                .enumerate()
                .map(|(i, row)| {
                    row.iter()
                        .enumerate()
                        .filter_map(|(j, chance)| {
                            if cells[i][j] == CellState::EMPTY {
                                Some(chance)
                            } else {
                                None
                            }
                        })
                        .max()
                })
                .max()
                .flatten()
                .copied()
                .unwrap_or_default()
        }
    };

    let cells = &mut GRID.lock().expect("Attempt to use locked mutex").cells;
    for (i, row) in chances.iter().enumerate() {
        for (j, chance) in row.iter().enumerate().take(inputs.grid_size) {
            let cell = document
                .get_element_by_id(&format!("{i}x{j}"))
                .expect("Could not find a grip element at expected index!");

            if cells[i][j] != CellState::EMPTY {
                cell.set_text_content(Some(&format!("{}", cells[i][j])));
                match cells[i][j] {
                    CellState::EMPTY => cell.set_class_name(""),
                    CellState::MISS => cell.set_class_name("miss"),
                    CellState::HIT => cell.set_class_name("hit"),
                    CellState::SUNK => cell.set_class_name("sunk"),
                }

                continue;
            }

            if max_cell_chance != 0 && *chance == max_cell_chance {
                cell.set_class_name("top-guess");
            } else {
                cell.set_class_name("");
            }

            if *chance == 0 {
                cell.set_text_content(Some(""));
            } else {
                cell.set_text_content(Some(&format!("{}", chance)));
            }
        }
    }
}

pub fn regenerate_grid() -> Result<(), JsValue> {
    let document = get_document();
    let grid = document
        .get_element_by_id("grid")
        .expect("Couldn't get grid");
    let inputs = get_inputs();

    GRID.lock().expect("Attempt to use locked mutex").cells =
        vec![vec![CellState::EMPTY; inputs.grid_size]; inputs.grid_size];

    let grid_header_row = document.create_element("tr")?;
    grid_header_row.append_child(document.create_element("th")?.as_ref())?;

    for i in 0..inputs.grid_size {
        let table_header = document.create_element("th")?;
        table_header.set_text_content(Some(&name_from_number(i)));
        grid_header_row.append_child(&table_header)?;
    }

    grid.append_child(&grid_header_row)?;

    for i in 0..inputs.grid_size {
        let grid_row = document.create_element("tr")?;
        let column_marker = document.create_element("th")?;
        column_marker.set_text_content(Some(&format!("{}", i + 1)));
        grid_row.append_child(&column_marker)?;

        for j in 0..inputs.grid_size {
            let cell = document.create_element("td")?;
            setup_cell(&cell, i, j)?;
            grid_row.append_child(&cell)?;
        }

        grid.append_child(&grid_row)?;
    }

    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_log::init_with_level(Level::Debug).expect("Failed to initialize the console logger");
    console_error_panic_hook::set_once();

    let document = get_document();
    let url = Url::parse(&document.url()?).expect("Failed to parse the URL");

    let grid_size_query_param =
        url.query_pairs()
            .find_map(|(param_name, value)| if param_name == "n" { Some(value) } else { None });
    let ships_query_param = url.query_pairs().find_map(|(param_name, value)| {
        if param_name == "ships" {
            Some(value)
        } else {
            None
        }
    });

    let react_to_input_change_closure = Closure::wrap(Box::new(move || {
        let document = get_document();

        let grid = document
            .get_element_by_id("grid")
            .expect("Couldn't get grid");

        let grid_parent = grid
            .parent_element()
            .expect("Failed to get the grid parent element");
        grid.remove();

        let new_grid = document
            .create_element("table")
            .expect("Failed to instantiate a table");
        new_grid.set_id("grid");
        grid_parent
            .append_child(&new_grid)
            .expect("Failed to attach the table");

        let inputs = get_inputs();
        let ships_str = inputs
            .ships
            .iter()
            .map(|x| format!("{x}"))
            .collect::<Vec<_>>()
            .join(" ");
        let url = window().expect("Failed to get the window").origin();

        window()
            .expect("Failed to get the window")
            .history()?
            .replace_state_with_url(
                &JsValue::UNDEFINED,
                "!!!",
                Some(&format!(
                    "{}?n={}&ships={}",
                    url, inputs.grid_size, ships_str
                )),
            )?;
        regenerate_grid()?;

        refresh();
        Ok(())
    })
        as Box<dyn FnMut() -> Result<(), JsValue>>);

    // Ships input
    {
        let ships_input = document
            .get_element_by_id("ships")
            .expect("Couldn't get ships input");

        ships_input.add_event_listener_with_callback(
            "change",
            react_to_input_change_closure.as_ref().unchecked_ref(),
        )?;

        if let Some(ships_query_param) = ships_query_param {
            ships_input
                .dyn_into::<HtmlInputElement>()?
                .set_value(&ships_query_param);
        }
    }

    // Grid size input
    {
        let grid_size_input = document
            .get_element_by_id("grid-size")
            .expect("Couldn't get grid-size input");

        grid_size_input.add_event_listener_with_callback(
            "change",
            react_to_input_change_closure.as_ref().unchecked_ref(),
        )?;

        if let Some(grid_size_query_param) = grid_size_query_param {
            grid_size_input
                .dyn_into::<HtmlInputElement>()?
                .set_value(&grid_size_query_param);
        }
    }

    react_to_input_change_closure.forget();

    regenerate_grid()?;
    refresh();

    Ok(())
}

pub fn main() {
    start().unwrap();
}
