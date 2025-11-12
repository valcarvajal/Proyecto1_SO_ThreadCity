use std::collections::{VecDeque, HashMap};
use crate::{Block, BlockKind, BlockTask, Coord, Direction, Directions, Matrix, VehicleKind, is_valid_position_for_vehicle};

/// Calcula una ruta usando BFS en la ciudad.
/// Devuelve un vector de coordenadas desde start hasta goal (incluyendo ambos).
pub fn bfs_path(
    city: &Matrix<Block>,
    start: Coord,
    goal: Coord,
    vehicle_kind: VehicleKind,
) -> Option<Vec<Coord>> {
    // Verificar si ya estamos en el goal o a 1 bloque de distancia
    if manhattan_distance(start, goal) <= 1 {
        return Some(vec![start]);
    }

    let mut queue = VecDeque::new();
    let mut visited: HashMap<Coord, Option<Coord>> = HashMap::new(); // nodo actual -> padre

    queue.push_back(start);
    visited.insert(start, None);

    pub fn direction_from_to(a: Coord, b: Coord) -> Option<Direction> {
        let dy = b.0 as isize - a.0 as isize;
        let dx = b.1 as isize - a.1 as isize;
        match (dy, dx) {
            (-1,  0) => Some(Direction::North),
            ( 1,  0) => Some(Direction::South),
            ( 0,  1) => Some(Direction::East),
            ( 0, -1) => Some(Direction::West),
            _        => None, // diagonal o salto de más de 1 celda: inválido
        }
    }

    // Función auxiliar para calcular distancia Manhattan
    fn manhattan_distance(a: Coord, b: Coord) -> usize {
        ((a.0 as isize - b.0 as isize).abs() + (a.1 as isize - b.1 as isize).abs()) as usize
    }

    while let Some(current) = queue.pop_front() {
        let (row, col) = current;
        let block: &Block = Matrix::get(city, row, col);

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

            let direction: Option<Direction> = direction_from_to(current, next);
            if !block.allows_direction(direction.unwrap()) {
                continue;
            }

            visited.insert(next, Some(current));

            // MODIFICACIÓN: Verificar si estamos a 1 bloque de distancia del goal
            if manhattan_distance(next, goal) <= 1 {
                let mut path = vec![next];
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

/// Función auxiliar para imprimir la ciudad con la ruta resaltada en rojo
fn print_path_on_city(city: &Matrix<Block>, path: &Vec<Coord>) {
    println!("\n Mapa con ruta marcada en ROJO:");
    println!("Leyenda: ");
    println!("'•' = Path, '■' = Building, '~' = River, '⌂' = Shop");
    println!("'☢' = NuclearPlant, '✙' = Hospital, '█' = Dock, '◉' = Spawn task \n ");
    println!("\x1b[31m'*'\x1b[0m = Ruta \n ");
    
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let coord = (row, col);
            
            // Si la coordenada está en la ruta, imprimir en rojo
            if path.contains(&coord) {
                print!("\x1b[31m*\x1b[0m "); // Carácter * en rojo
                continue;
            }
            
            let block = Matrix::get(city, row, col);
            let symbol = match block.kind {
                BlockKind::Path => "•",
                BlockKind::Building => "■",
                BlockKind::River => "~",
                BlockKind::Shop => "⌂",
                BlockKind::NuclearPlant => "☢",
                BlockKind::Hospital => "✙",
                BlockKind::Dock => "█",
            };

            
            
            // Mostrar otros
            if block.task == Some(BlockTask::Spawn) { 
                print!("◉ "); 
            }
            else if block.dirs == Directions::north() { 
                print!("↑ "); 
            }
            else if block.dirs == Directions::south() { 
                print!("↓ "); 
            }
            else if block.dirs == Directions::east()  { 
                print!("→ "); 
            }
            else if block.dirs == Directions::west()  { 
                print!("← "); 
            }
            else if block.dirs == Directions::north_east()  { 
                print!("↗ "); 
            }
            else if block.dirs == Directions::north_west()  { 
                print!("↖ "); 
            }
            else if block.dirs == Directions::south_east()  { 
                print!("↘ "); 
            }
            else if block.dirs == Directions::south_west()  { 
                print!("↙ "); 
            }
            else if block.dirs == Directions::north_south_west()  { 
                print!("◁ "); 
            }
            else {
                print!("{} ", symbol);
            }
        }
        println!();
    }
}