use std::collections::{VecDeque, HashMap};
use crate::{Matrix, Block, Coord, VehicleKind, is_valid_position_for_vehicle};

/// Calcula una ruta usando BFS en la ciudad.
/// Devuelve un vector de coordenadas desde start hasta goal (incluyendo ambos).
pub fn bfs_path(
    city: &Matrix<Block>,
    start: Coord,
    goal: Coord,
    vehicle_kind: VehicleKind,
) -> Option<Vec<Coord>> {
    if start == goal {
        return Some(vec![start]);
    }

    let mut queue = VecDeque::new();
    let mut visited: HashMap<Coord, Option<Coord>> = HashMap::new(); // nodo actual -> padre

    queue.push_back(start);
    visited.insert(start, None);

    while let Some(current) = queue.pop_front() {
        let (row, col) = current;

        // Generar vecinos (arriba, abajo, derecha, izquierda)
        let dirs = [(-1, 0), (1, 0), (0, 1), (0, -1)];

        for (dr, dc) in dirs {
            let new_row = row as isize + dr;
            let new_col = col as isize + dc;

            if new_row < 0
                || new_row >= city.rows() as isize
                || new_col < 0
                || new_col >= city.cols() as isize
            {
                continue;
            }

            let next = (new_row as usize, new_col as usize);

            if visited.contains_key(&next) {
                continue;
            }

            if !is_valid_position_for_vehicle(city, next, vehicle_kind) {
                continue;
            }

            visited.insert(next, Some(current));

            if next == goal {
                let mut path = vec![goal];
                let mut p = Some(current);
                while let Some(prev) = p {
                    path.push(prev);
                    p = visited[&prev];
                }
                path.reverse();

                println!("Ruta encontrada ({} pasos):", path.len());
                for (i, (r, c)) in path.iter().enumerate() {
                    println!("  Paso {:>2}: ({}, {})", i, r, c);
                }

                print_path_on_city(city, &path);
                return Some(path);
            }

            queue.push_back(next);
        }
    }

    println!("⚠️ No se encontró una ruta válida desde {:?} hasta {:?}.", start, goal);
    None
}

fn print_path_on_city(city: &Matrix<Block>, path: &Vec<Coord>) {
    let mut display = vec![vec![' '; city.cols()]; city.rows()];

    for r in 0..city.rows() {
        for c in 0..city.cols() {
            if is_valid_position_for_vehicle(city, (r, c), VehicleKind::Car) {
                display[r][c] = '.';
            } else {
                display[r][c] = '■';
            }
        }
    }

    for &(r, c) in path {
        display[r][c] = '*';
    }

    println!("\n🗺️ Mapa con ruta marcada:\n");
    for row in display {
        for ch in row {
            print!("{}", ch);
        }
        println!();
    }
}
