use std::collections::{VecDeque, HashMap};
use crate::{Matrix, Block, Coord, VehicleKind, Direction};
use crate::is_valid_position_for_vehicle;

/// Calcula una ruta usando BFS considerando las direcciones válidas de cada bloque.
/// Devuelve un vector de coordenadas desde `start` hasta `goal` (incluyendo ambos).
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
    let mut visited: HashMap<Coord, Option<Coord>> = HashMap::new(); // nodo -> padre

    queue.push_back(start);
    visited.insert(start, None);

    while let Some(current) = queue.pop_front() {
        let (row, col) = current;
        let block = city.get(row, col);

        // Generar vecinos según las direcciones válidas del bloque
        let mut neighbors = Vec::new();
        if block.get_north() && row > 0 {
            neighbors.push((row - 1, col));
        }
        if block.get_south() && row + 1 < city.rows() {
            neighbors.push((row + 1, col));
        }
        if block.get_east() && col + 1 < city.cols() {
            neighbors.push((row, col + 1));
        }
        if block.get_west() && col > 0 {
            neighbors.push((row, col - 1));
        }

        for next in neighbors {
            if visited.contains_key(&next) {
                continue;
            }

            if !is_valid_position_for_vehicle(city, next, vehicle_kind) {
                continue;
            }

            visited.insert(next, Some(current));

            if next == goal {
                // reconstruir el camino
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

/// Dibuja visualmente la ruta sobre la ciudad en la consola.
fn print_path_on_city(city: &Matrix<Block>, path: &Vec<Coord>) {
    let mut display = vec![vec![' '; city.cols()]; city.rows()];

    for r in 0..city.rows() {
        for c in 0..city.cols() {
            let block = city.get(r, c);
            display[r][c] = match block.kind {
                crate::BlockKind::Building => '■',
                crate::BlockKind::River => '~',
                crate::BlockKind::Shop => '⌂',
                crate::BlockKind::NuclearPlant => '☢',
                crate::BlockKind::Hospital => '✙',
                crate::BlockKind::Dock => '█',
                crate::BlockKind::Path => '•',
            };
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
