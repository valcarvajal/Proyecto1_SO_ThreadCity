use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use mypthreads::*;
use rmatrix::*;
mod bfs;
mod city_design;
use bfs::bfs_path;
use rand;
use rand::Rng;
use std::ffi::c_void;
use std::{fmt, ptr};
use std::ptr::null_mut;
use std::time::Duration;

use crate::city_design::CITY_DESIGN;

/// --------------------------------------------------------------------------- ///
///                                 Vehiculos                                   ///
/// --------------------------------------------------------------------------- ///

/// Coordenada (x, y) en la grid: x = columna, y = fila.
pub type Coord = (usize, usize);

/// ID lógico de vehículo dentro de la simulación.
pub type VehicleId = usize;

// Número máximo de vehículos en la simulación al mismo tiempo
pub const MAX_VEHICLES: usize = 10;

// Número de vehículos totales a simular
pub const TOTAL_VEHICLES: usize = 25;

pub static mut COUNT: usize = 0;

/// Tipos de vehículos
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub enum VehicleKind {
    Car,               // carro normal
    Ambulance,         // ambulancia
    TruckWater,        // camión de agua
    TruckRadioactive,  // camión de material radiactivo
    Boat,              // barco
}

impl fmt::Display for VehicleKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Struct de vehículo.
#[derive(Debug)]
pub struct Vehicle {
    id: VehicleId,
    kind: VehicleKind,
    route: Vec<Coord>,  // incluye posición inicial y todos los pasos
}

impl Vehicle {
    pub fn new(id: VehicleId, kind: VehicleKind, start: Coord, dest: Coord, city: &City) -> Self {
        let r = bfs_path(city, start, dest, kind);
        Vehicle {
            id,
            kind,
            route: r.unwrap_or_else(|| vec![]),
        }
    }
}

extern "C" fn vehicle_thread(arg: *mut c_void) -> *mut c_void {
    unsafe {
        // Recuperar y tomar propiedad de los argumentos
        let mut boxed_args: Box<Vehicle> = Box::from_raw(arg as *mut Vehicle);
        let id   = boxed_args.id;
        let kind = boxed_args.kind;
        let mut route = std::mem::take(&mut boxed_args.route);
        let count = 0;
        drop(boxed_args);

        if route.is_empty() {
            println!("[{} {}] Ruta vacía, terminando.", kind.to_string(), id);
            return ptr::null_mut();
        }

        // Posición inicial
        let mut pos = route.remove(0);

        // Tomar lock de la celda inicial y marcar ocupante
        {
            let city_ref = city();
            let block = city_ref.get_mut(pos.0, pos.1);
            block.lock_block();
            block.set_occupant(Some(id));
        }

        println!("[{} {}] Inicia en {:?}, destino {:?}", kind.to_string(), id, pos, route.last());

        // Recorrer la ruta
        while let Some(next_pos) = route.first().copied() {
            // 1) Verificar que next_pos es vecino directo y respeta la dirección del bloque actual
            let dir = match direction_from_to(pos, next_pos) {
                Some(d) => d,
                None => {
                    println!(
                        "[{} {}] ERROR: {:?} no es vecino directo de {:?}, abortando ruta.",
                        kind.to_string(), id, next_pos, pos
                    );
                    break;
                }
            };

            {
                let city_ref = city();
                let curr_block = city_ref.get(pos.0, pos.1);
                if !curr_block.allows_direction(dir) {
                    println!(
                        "[{} {}] ERROR: intento mover {:?} -> {:?} en dirección {} pero el bloque no lo permite, abortando ruta.",
                        kind.to_string(), id, pos, next_pos, dir.to_string(),
                    );
                    break;
                }
            }

            // 2) Intentar tomar el lock del bloque destino SIN bloquear (para detectar contención)
            let rc = {
                let city_ref = city();
                let next_block_ptr = city_ref.get_mut(next_pos.0, next_pos.1) as *mut Block;
                my_mutex_trylock(&mut (*next_block_ptr).lock)
            };

            if rc != 0 {
                // Condición de carrera / contención sobre el recurso (bloque destino)
                println!(
                    "[RACE] {} {} quiere entrar a {:?} (dir {}) pero el recurso está ocupado; \
scheduler prioriza a otro vehículo mientras este hilo cede CPU.",
                    kind.to_string(),
                    id,
                    next_pos,
                    dir.to_string(),
                );

                // Ceder CPU explícitamente: aquí el scheduler (RR/Lottery/RT) decide a quién correr
                my_thread_yield();
                continue;
            }

            // 3) Tenemos lock de destino + todavía mantenemos lock de origen
            //    Actualizar ocupantes y liberar lock de origen.
            {
                let city_ref = city();

                let curr_block_ptr = city_ref.get_mut(pos.0, pos.1) as *mut Block;
                let next_block_ptr = city_ref.get_mut(next_pos.0, next_pos.1) as *mut Block;

                // Por seguridad, verificar que destino no tenía ocupante
                if (*next_block_ptr).get_occupant().is_some() {
                    println!(
                        "[{} {}] WARNING: bloque {:?} ya tenía ocupante a pesar del lock, liberando y reintentando.",
                        kind.to_string(), id, next_pos
                    );
                    my_mutex_unlock(&mut (*next_block_ptr).lock);
                    my_thread_yield();
                    continue;
                }

                (*next_block_ptr).set_occupant(Some(id));
                (*curr_block_ptr).set_occupant(None);
                my_mutex_unlock(&mut (*curr_block_ptr).lock);
            }

            // 4) Loguear movimiento con dirección
            println!(
                "[{} {}] Mueve {:?} -> {:?} hacia {}",
                kind.to_string(),
                id,
                pos,
                next_pos,
                dir.to_string(),
            );

            // Actualizar posición y seguir con la ruta
            pos = next_pos;
            route.remove(0);

            // 5) Ceder CPU para que otros vehículos se muevan
            my_thread_yield();
        }

        // Limpiar última celda
        {
            let city_ref = city();
            let last_block = city_ref.get_mut(pos.0, pos.1);
            last_block.set_occupant(None);
            last_block.unlock_block();
        }

        println!("[{} {}] Terminado en {:?}", kind, id, pos);
        ptr::null_mut()
    }
}

/// --------------------------------------------------------------------------- ///
///                                  Ciudad                                     ///
/// --------------------------------------------------------------------------- ///



#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Path,          // carreteras y puentes
    Building,      // construcciones
    River,         // río
    Shop,          // tiendas
    NuclearPlant,  // parte de planta nuclear
    Hospital,      // parte de hospital
    Dock,          // atracadero
}

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub enum BlockTask {
    Spawn,        // punto de salida
    TrafficLight, // semáforo
    Yield,        // ceda el paso
    Drawbridge,   // puente levadizo
}

#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub struct Directions {
    north: bool,
    south: bool, 
    east: bool,
    west: bool,
}

impl Directions {
    pub fn north() -> Self {
        Directions { north: true, south: false, east: false, west: false }
    }
    
    pub fn south() -> Self {
        Directions { north: false, south: true, east: false, west: false }
    }
    
    pub fn east() -> Self {
        Directions { north: false, south: false, east: true, west: false }
    }
    
    pub fn west() -> Self {
        Directions { north: false, south: false, east: false, west: true }
    }
    
    pub fn north_east() -> Self {
        Directions { north: true, south: false, east: true, west: false }
    }
    
    pub fn north_west() -> Self {
        Directions { north: true, south: false, east: false, west: true }
    }
    
    pub fn south_east() -> Self {
        Directions { north: false, south: true, east: true, west: false }
    }
    
    pub fn south_west() -> Self {
        Directions { north: false, south: true, east: false, west: true }
    }

    pub fn north_south_west() -> Self {
        Directions { north: true, south: true, east: false, west: true }
    }
    
    pub fn none() -> Self {
        Directions { north: false, south: false, east: false, west: false }
    }
}

// Enum adicional para direcciones
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub task: Option<BlockTask>,        // None si el bloque no tiene tarea especial
    pub dirs: Directions,               // direcciones válidas desde este bloque
    pub occupant: Option<VehicleId>,
    pub lock: MyMutex,
}

impl Block {

    // Constructor

    pub fn new() -> Self {
        Block {
            kind: BlockKind::Path,
            task: None,
            dirs: Directions {
                north: false,
                south: false,
                east: false,
                west: false,
            },
            occupant: None,
            lock: MyMutex::new(),
        }
    }

    // Métodos GET para atributos generales

    pub fn get_kind(&self) -> BlockKind {
        self.kind
    }

    pub fn get_task(&self) -> Option<BlockTask> {
        self.task
    }

    pub fn get_occupant(&self) -> Option<VehicleId> {
        self.occupant
    }

    pub fn get_lock(&self) -> &MyMutex {    
        &self.lock
    }

    // Métodos SET para atributos generales

    pub fn set_kind(&mut self, kind: BlockKind) {
        self.kind = kind;
    }

    pub fn set_task(&mut self, task: Option<BlockTask>) {
        self.task = task;
    }

    pub fn set_occupant(&mut self, occupant: Option<VehicleId>) {
        self.occupant = occupant;
    }

    pub fn set_lock(&mut self, lock: MyMutex) {
        self.lock = lock;
    }

    // Métodos para bloquear/desbloquear el mutex del bloque

    pub fn lock_block(&mut self) {
        my_mutex_lock(&mut self.lock);
    }

    pub fn unlock_block(&mut self) {
        my_mutex_unlock(&mut self.lock);
    }

    // Métodos GET para cada dirección

    pub fn get_directions(&self) -> Directions {
        self.dirs
    }

    pub fn get_north(&self) -> bool {
        self.dirs.north
    }
    
    pub fn get_south(&self) -> bool {
        self.dirs.south
    }
    
    pub fn get_east(&self) -> bool {
        self.dirs.east
    }
    
    pub fn get_west(&self) -> bool {
        self.dirs.west
    }
    
    // Métodos SET para cada dirección

    pub fn set_directions(&mut self, directions: Directions) {
        self.dirs = directions;
    }

    pub fn set_north(&mut self, value: bool) {
        self.dirs.north = value;
    }
    
    pub fn set_south(&mut self, value: bool) {
        self.dirs.south = value;
    }
    
    pub fn set_east(&mut self, value: bool) {
        self.dirs.east = value;
    }
    
    pub fn set_west(&mut self, value: bool) {
        self.dirs.west = value;
    }
    
    // Método para verificar si una dirección es válida

    pub fn allows_direction(&self, direction: Direction) -> bool {
        match direction {
            Direction::North => self.get_north(),
            Direction::South => self.get_south(),
            Direction::East => self.get_east(),
            Direction::West => self.get_west(),
        }
    }
    
}

impl Default for Block {
    fn default() -> Self {
        Block {
            kind: BlockKind::Path,
            task: None,
            dirs: Directions {
                north: false,
                south: false,
                east: false,
                west: false,
            },
            occupant: None,
            lock: MyMutex::new(),
        }
    }
}

impl Clone for Block {
    fn clone(&self) -> Self {
        Block {
            kind: self.kind,
            task: self.task,
            dirs: self.dirs,
            occupant: None,
            lock: MyMutex::new(),
        }
    }
}

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

pub type City = Matrix<Block>;

/// Crea una ciudad con el patrón especificado
pub fn build_city() -> City {

    let mut height = city_design::GRID_HEIGHT;
    let mut width = city_design::GRID_WIDTH;
    let mut design = CITY_DESIGN;
    let mut city = City::new(height, width);

    // 1) Setear kind y directions.
    for row in 0..height {
        for col in 0..width {

            let kind = match design[row][col] {
                '↑' | '↓' | '→' | '←' | '↗' | '↖' | '↘' | '↙' | '◁' => BlockKind::Path,
                'b' => BlockKind::Building,
                'r' => BlockKind::River,
                's' => BlockKind::Shop,
                'n' => BlockKind::NuclearPlant,
                'h' => BlockKind::Hospital,
                'd' => BlockKind::Dock,
                _   => BlockKind::Path,
            };

            let directions = match design[row][col] {
                '↑' => Directions::north(),
                '↓' => Directions::south(),
                '→' => Directions::east(),
                '←' => Directions::west(),
                '↗' => Directions::north_east(),
                '↖' => Directions::north_west(),
                '↘' => Directions::south_east(),
                '↙' => Directions::south_west(),
                '◁' => Directions::north_south_west(),
                _   => Directions::none(),
            };

            let mut block = Block::new();
            block.kind = kind;
            block.dirs = directions;
            city.set(row, col, block);
        }
    }

    // 2) Marcar puntos de spawn
    let spawn_candidates = [
        (0, 0), (0, 6), (0, 9), (0, 15),               // Borde superior
        (19, 0), (19, 6), (19, 9), (19, 15),           // Borde inferior
        (3, 0), (6, 0), (9, 0), (13, 0), (16, 0),      // Borde izquierdo
        (3, 15), (6, 15), (9, 15), (13, 15), (16, 15), // Borde derecho
    ];

    for &(row, col) in &spawn_candidates {
        if row < city.rows() && col < city.cols() {
            let block = city.get_mut(row, col);
            if block.kind == BlockKind::Path {
                block.task = Some(BlockTask::Spawn);
            }
        }
    }

    city

}

static mut CITY_PTR: *mut City = null_mut();

fn city() -> &'static mut City {
    unsafe {
        if CITY_PTR.is_null() {
            panic!("CITY_PTR no inicializado");
        }
        &mut *CITY_PTR
    }
}

/// Función auxiliar para imprimir la ciudad de forma legible
pub fn print_detailed_city(city: &Matrix<Block>) {
    println!("Mapa detallado de la ciudad ({}x{}):", city.rows(), city.cols());
    println!("Leyenda: ");
    println!("'•' = Path, '■' = Building, '~' = River, '⌂' = Shop");
    println!("'☢' = NuclearPlant, '✙' = Hospital, '█' = Dock, '◉' = Spawn task");
    
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
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
            if block.task == Some(BlockTask::Spawn) { print!("◉ "); }
            else if block.dirs == Directions::north() { print!("↑ "); }
            else if block.dirs == Directions::south() { print!("↓ "); }
            else if block.dirs == Directions::east()  { print!("→ "); }
            else if block.dirs == Directions::west()  { print!("← "); }
            else if block.dirs == Directions::north_east()  { print!("↗ "); }
            else if block.dirs == Directions::north_west()  { print!("↖ "); }
            else if block.dirs == Directions::south_east()  { print!("↘ "); }
            else if block.dirs == Directions::south_west()  { print!("↙ "); }
            else if block.dirs == Directions::north_south_west()  { print!("◁ "); }
            else {
                print!("{} ", symbol);
            }
        }
        println!();
    }
}



/// --------------------------------------------------------------------------- ///

/// Función para contar bloques por tipo
pub fn count_blocks_by_kind(city: &Matrix<Block>) -> HashMap<BlockKind, usize> {
    let mut counter = HashMap::new();
    
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let kind = city.get(row, col).kind;
            *counter.entry(kind).or_insert(0) += 1;
        }
    }
    
    counter
}

/// Encuentra posiciones de spawn (podrías agregar algunas después)
pub fn find_spawn_positions(city: &Matrix<Block>) -> Vec<Coord> {
    let mut positions = Vec::new();
    
    // Buscar en los bordes de Path para spawn points
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::Path && block.task == Some(BlockTask::Spawn) {
                positions.push((row, col));
            }
        }
    }
    
    positions
}

/// Encuentra las tiendas en la ciudad
pub fn find_shops(city: &Matrix<Block>) -> Vec<Coord> {
    let mut coords: Vec<Coord> = Vec::new();
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::Shop {
                coords.push((row, col));
            }
        }
    }
    coords
}

pub fn find_hospitals(city: &Matrix<Block>) -> Vec<Coord> {
    let mut coords: Vec<Coord> = Vec::new();
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::Hospital {
                coords.push((row, col));
            }
        }
    }
    coords
}

pub fn find_nuclear_plants(city: &Matrix<Block>) -> Vec<Coord> {
    let mut coords: Vec<Coord> = Vec::new();
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::NuclearPlant {
                coords.push((row, col));
            }
        }
    }
    coords
}

pub fn find_docks(city: &Matrix<Block>) -> Vec<Coord> {
    let mut coords: Vec<Coord> = Vec::new();
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::Dock {
                coords.push((row, col));
            }
        }
    }
    coords
}

/// Verifica si una coordenada es válida para un tipo de vehículo
pub fn is_valid_position_for_vehicle(city: &Matrix<Block>, pos: Coord, vehicle_kind: VehicleKind) -> bool {
    let (row, col) = pos;
    if row >= city.rows() || col >= city.cols() {
        return false;
    }
    
    let block = city.get(row, col);
    
    match vehicle_kind {
        VehicleKind::Car | VehicleKind::Ambulance | VehicleKind::TruckWater | VehicleKind::TruckRadioactive => {
            matches!(block.kind, BlockKind::Path | BlockKind::Shop | BlockKind::Hospital | BlockKind::NuclearPlant)
        }
        VehicleKind::Boat => {
            matches!(block.kind, BlockKind::River | BlockKind::Dock)
        }
    }
}

pub fn call_car(id : VehicleId) -> usize {
    let spawns = find_spawn_positions(&city());
    let shops = find_shops(&city());

    let spawnplace = rand::thread_rng().gen_range(0..spawns.len());
    let shopsplace = rand::thread_rng().gen_range(0..shops.len());

    let vehicle = Vehicle::new(id, VehicleKind::Car, spawns[spawnplace], shops[shopsplace], city());
    
    let boxed = Box::new(vehicle);
    let arg_ptr = Box::into_raw(boxed) as *mut c_void;

    let policy: SchedPolicy = SchedPolicy::RoundRobin;

    let tid = my_thread_create(vehicle_thread, arg_ptr, policy);

    println!("[MAIN] Creado carro {} con tid {} y política {:?}", id, tid, policy);

    tid
}

pub fn call_ambulance(id : VehicleId) -> usize {
    let spawns = find_spawn_positions(&city());
    let hospitals = find_hospitals(&city());

    let spawnplace = rand::thread_rng().gen_range(0..spawns.len());
    let hospitalsplace = rand::thread_rng().gen_range(0..hospitals.len());

    let vehicle = Vehicle::new(id, VehicleKind::Ambulance, spawns[spawnplace], hospitals[hospitalsplace], city());
    
    let boxed = Box::new(vehicle);
    let arg_ptr = Box::into_raw(boxed) as *mut c_void;

    let policy: SchedPolicy = SchedPolicy::Lottery { tickets: 50 };

    let tid = my_thread_create(vehicle_thread, arg_ptr, policy);

    println!("[MAIN] Creado ambulancia {} con tid {} y política {:?}", id, tid, policy);

    tid
}

pub fn call_truck_water(id : VehicleId, deadline: u64) -> usize {
    let spawns = find_spawn_positions(&city());
    let nuclear_plants = find_nuclear_plants(&city());

    let spawnplace = rand::thread_rng().gen_range(0..spawns.len());
    let nuclear_plants_place = rand::thread_rng().gen_range(0..nuclear_plants.len());

    let vehicle = Vehicle::new(id, VehicleKind::TruckWater, spawns[spawnplace], nuclear_plants[nuclear_plants_place], city());

    let boxed = Box::new(vehicle);
    let arg_ptr = Box::into_raw(boxed) as *mut c_void;

    let policy: SchedPolicy = SchedPolicy::RealTime { deadline };

    let tid = my_thread_create(vehicle_thread, arg_ptr, policy);

    println!("[MAIN] Creado camión de agua {} con tid {} y política {:?}", id, tid, policy);

    tid
}
pub fn call_truck_radioactive(id : VehicleId, deadline: u64) -> usize {
    let spawns = find_spawn_positions(&city());
    let nuclear_plants = find_nuclear_plants(&city());

    let spawnplace = rand::thread_rng().gen_range(0..spawns.len());
    let nuclear_plants_place = rand::thread_rng().gen_range(0..nuclear_plants.len());

    let vehicle = Vehicle::new(id, VehicleKind::TruckRadioactive, spawns[spawnplace], nuclear_plants[nuclear_plants_place], city());

    let boxed = Box::new(vehicle);
    let arg_ptr = Box::into_raw(boxed) as *mut c_void;

    let policy: SchedPolicy = SchedPolicy::RealTime { deadline };

    let tid = my_thread_create(vehicle_thread, arg_ptr, policy);

    println!("[MAIN] Creado camión radioactivo {} con tid {} y política {:?}", id, tid, policy);

    tid
}

fn run_simulation() {

    let mut cars = Vec::new(); // Vector para almacenar los resultados

    for i in 1..=15 {
        cars.push(call_car(i));
    }

    let mut ambulances = Vec::new();
    for i in 15..=21 {
        ambulances.push(call_ambulance(i));
    }

    let truck_water1 = call_truck_water(22, 15);
    let truck_radioactive1 = call_truck_radioactive(23, 10);

    let tids1 = vec![
        cars,
        ambulances,
        vec![truck_water1, truck_radioactive1],
    ].concat();

    // Esperar a que terminen vehículos
    for tid in tids1 {
        my_thread_join(tid);
    }

    let truck_water2 = call_truck_water(24, 8);
    let truck_radioactive2 = call_truck_radioactive(25, 12);

    let tids2 = vec![truck_water2, truck_radioactive2];

        // Esperar a que terminen vehículos
    for tid in tids2 {
        my_thread_join(tid);
    }

    println!("[MAIN] Todos los vehículos de prueba han terminado.");
}

/// --------------------------------------------------------------------------- ///
///                                  Ejecución                                  ///
/// --------------------------------------------------------------------------- ///

fn main() {

    // Crear ciudad
    let city_box = Box::new(build_city());
    unsafe { CITY_PTR = Box::into_raw(city_box); }
    let city = city();
    print_detailed_city(&city);

    let kind_stats = count_blocks_by_kind(city);
    let spawn_positions = find_spawn_positions(city);

    println!("\n=== ESTADÍSTICAS DE LA CIUDAD ===");
    println!("\nPor tipo de bloque:");
    for (kind, count) in kind_stats {
        let kind_name = match kind {
            BlockKind::Path => "Path",
            BlockKind::Building => "Building",
            BlockKind::River => "River",
            BlockKind::Shop => "Shop",
            BlockKind::NuclearPlant => "NuclearPlant",
            BlockKind::Hospital => "Hospital",
            BlockKind::Dock => "Dock",
        };
        println!("  {}: {}", kind_name, count);
    }

    println!("Posiciones de spawn: {}", spawn_positions.len());

    println!("\n=== VALIDACIÓN DE VEHÍCULOS ===");
    let test_positions = [(0, 0), (10, 0), (12, 8), (4, 4)];
    for &pos in &test_positions {
        println!("\nPosición {:?}:", pos);
        for vehicle_kind in [
            VehicleKind::Car,
            VehicleKind::Ambulance,
            VehicleKind::TruckWater,
            VehicleKind::TruckRadioactive,
            VehicleKind::Boat,
        ].iter()
        {
            let is_valid = is_valid_position_for_vehicle(city, pos, *vehicle_kind);
            println!("  {:?}: {}", vehicle_kind, is_valid);
        }
    }

    // Aquí lanzamos la simulacion completa
    run_simulation();
}
