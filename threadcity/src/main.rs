use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use mypthreads::*;
use rmatrix::*;
mod bfs;
use bfs::bfs_path;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::ffi::c_void;
use std::ptr;
use std::time::Duration;
use std::thread::sleep;

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

/// Tipos de vehículos
#[derive(Copy, Clone, Hash, Debug, PartialEq, Eq)]
pub enum VehicleKind {
    Car,               // carro normal
    Ambulance,         // ambulancia
    TruckWater,        // camión de agua
    TruckRadioactive,  // camión de material radiactivo
    Boat,              // barco
}

/// Struct de vehículo.
#[derive(Debug)]
pub struct Vehicle {
    pub id: VehicleId,
    pub kind: VehicleKind,
    pub pos: Coord,
    pub dest: Coord,                     // bloque destino a alcanzar
    pub route: Vec<Coord>,               // ruta planificada (lista de bloques)
    pub thread_id: Option<MyThreadId>,   // hilo mypthread que lo controla
}

impl Vehicle {

    pub fn run(&mut self) {
        while self.pos != self.dest {
            self.pos = self.route[0];
            self.route.remove(0); 
        }
    }

    // Constructor

    pub fn new(id: VehicleId, kind: VehicleKind, pos: Coord, dest: Coord, schedpolicy: SchedPolicy) -> Self {
        Vehicle {
            id,
            kind,
            pos,
            dest,
            route: Vec::new(),
            thread_id: None,
        }
    }
}

/// --------------------------------------------------------------------------- ///
///                                  Ciudad                                     ///
/// --------------------------------------------------------------------------- ///

/// Ancho y altura de la grid de la ciudad.
pub const GRID_WIDTH: usize = 16;
pub const GRID_HEIGHT: usize = 20;

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
struct Directions {
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

#[derive(Debug)]
struct Block {
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

/// Crea una ciudad con el patrón especificado
pub fn build_city() -> Matrix<Block> {
    let mut city = Matrix::<Block>::new(GRID_HEIGHT, GRID_WIDTH);

    // Patrón detallado basado en tu especificación
    let pattern: [[char; GRID_WIDTH]; GRID_HEIGHT] = [
        // 0
        ['→', '→', '→', '↘', '→', '→', '→', '→', '→', '↘', '→', '→', '→', '→', '→', '↓'],
        // 1
        ['↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓'],
        // 2
        ['↑', 'b', 'b', '↓', 'b', 's', '↑', 'b', 'b', '↓', 's', 'b', '↑', 'b', 'b', '↓'],
        // 3
        ['↗', '→', '→', '↘', '→', '→', '↗', '→', '→', '↘', '→', '→', '↗', '→', '→', '↓'],
        // 4
        ['↑', 'b', 'b', '↓', 'n', 'n', '↑', 'b', 's', '↓', 'h', 'h', '↑', 'b', 'b', '↓'],
        // 5
        ['↑', 'b', 'b', '↓', 'n', 'n', '↑', 's', 'b', '↓', 'h', 'h', '↑', 'b', 'b', '↓'],
        // 6
        ['↑', '←', '←', '↙', '←', '←', '↖', '←', '←', '↙', '←', '←', '↖', '←', '←', '↙'],
        // 7
        ['↑', 'b', 'b', '↓', 'b', 's', '↑', 'b', 'b', '↓', 's', 'b', '↑', 'b', 'b', '↓'],
        // 8
        ['↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓'],
        // 9
        ['↑', '←', '←', '↙', '←', '←', '◁', '←', '←', '↙', '←', '←', '◁', '←', '←', '←'],
        // 10
        ['r', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', 'r'],
        // 11
        ['r', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'r', 'r'],
        // 12
        ['r', 'r', 'r', '↓', 'r', 'r', '↓', 'r', 'd', '↓', 'r', 'r', '↓', 'r', 'r', 'r'],
        // 13
        ['→', '→', '→', '↘', '→', '→', '→', '→', '→', '↘', '→', '→', '→', '→', '→', '↓'],
        // 14
        ['↑', 'b', 'b', '↓', 'b', 'b', '↑', 'n', 'n', '↓', 'b', 'b', '↑', 'b', 'b', '↓'],
        // 15
        ['↑', 'b', 'b', '↓', 's', 'b', '↑', 'n', 'n', '↓', 'b', 's', '↑', 'b', 'b', '↓'],
        // 16
        ['↗', '→', '→', '↘', '→', '→', '↗', '→', '→', '↘', '→', '→', '↗', '→', '→', '↓'],
        // 17
        ['↑', 'b', 'b', '↓', 'b', 's', '↑', 'b', 'b', '↓', 's', 'b', '↑', 'b', 'b', '↓'],
        // 18
        ['↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓', 'b', 'b', '↑', 'b', 'b', '↓'],
        // 19
        ['↑', '←', '←', '←', '←', '←', '↖', '←', '←', '←', '←', '←', '↖', '←', '←', '←'],
    ];

    // 1) Setear kind y directions.
    for row in 0..GRID_HEIGHT {
        for col in 0..GRID_WIDTH {

            let kind = match pattern[row][col] {
                '↑' | '↓' | '→' | '←' | '↗' | '↖' | '↘' | '↙' | '◁' => BlockKind::Path,
                'b' => BlockKind::Building,
                'r' => BlockKind::River,
                's' => BlockKind::Shop,
                'n' => BlockKind::NuclearPlant,
                'h' => BlockKind::Hospital,
                'd' => BlockKind::Dock,
                _   => BlockKind::Path,
            };

            let directions = match pattern[row][col] {
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

/// Función que ejecuta cada hilo de vehículo.
/// El argumento será un puntero a un Vehicle.
extern "C" fn vehicle_thread(arg: *mut c_void) -> *mut c_void {
    unsafe {
        // Recuperar el vehículo desde el puntero
        let vehicle: &mut Vehicle = &mut *(arg as *mut Vehicle);

        println!("🚗 Iniciando vehículo {:?} desde {:?} hacia {:?}", vehicle.kind, vehicle.pos, vehicle.dest);

        while let Some(next_pos) = vehicle.route.first().cloned() {
            // Simular movimiento: bloquear siguiente bloque
            println!("   -> Vehículo {:?} moviéndose a {:?}", vehicle.kind, next_pos);
            vehicle.pos = next_pos;
            vehicle.route.remove(0);

            // Simular tiempo de desplazamiento (varía por tipo)
            let delay = match vehicle.kind {
                VehicleKind::Ambulance => 200,
                VehicleKind::Car => 400,
                VehicleKind::TruckWater | VehicleKind::TruckRadioactive => 600,
                VehicleKind::Boat => 800,
            };
            my_thread_yield(); // cede CPU al scheduler cooperativo
            sleep(Duration::from_millis(delay));
        }

        println!("✅ Vehículo {:?} llegó a destino {:?}", vehicle.kind, vehicle.dest);

        ptr::null_mut()
    }
}

/// --------------------------------------------------------------------------- ///
///                                  Ejecución                                  ///
/// --------------------------------------------------------------------------- ///

fn main() {
    println!("🏙️ Iniciando simulación ThreadCity...");

    // Crear ciudad
    let city = build_city();
    print_detailed_city(&city);

    // Obtener puntos de spawn
    let spawn_points = find_spawn_positions(&city);
    let mut rng = thread_rng();

    let mut vehicles: Vec<Vehicle> = Vec::new();

    // Crear 5 vehículos de distintos tipos
    let vehicle_types = [
        VehicleKind::Car,
        VehicleKind::Ambulance,
        VehicleKind::TruckWater,
        VehicleKind::TruckRadioactive,
        VehicleKind::Boat,
    ];

    for (i, kind) in vehicle_types.iter().enumerate() {
        // Elegir inicio y fin distintos
        let start = *spawn_points.choose(&mut rng).unwrap();
        let mut goal = *spawn_points.choose(&mut rng).unwrap();
        while goal == start {
            goal = *spawn_points.choose(&mut rng).unwrap();
        }

        // Calcular ruta con BFS
        if let Some(path) = bfs_path(&city, start, goal, *kind) {
            let mut vehicle = Vehicle::new(i, *kind, start, goal, SchedPolicy::RoundRobin);
            vehicle.route = path;
            vehicles.push(vehicle);
        } else {
            println!("⚠️ No se encontró ruta para vehículo {:?}", kind);
        }
    }

    println!("\n🚀 Lanzando hilos de vehículos...");

    // Crear hilos para cada vehículo
    for vehicle in vehicles.iter_mut() {
        let ptr = vehicle as *mut Vehicle as *mut c_void;
        let policy = match vehicle.kind {
            VehicleKind::Ambulance => SchedPolicy::RealTime { deadline: 5000 },
            VehicleKind::TruckRadioactive => SchedPolicy::Lottery { tickets: 5 },
            _ => SchedPolicy::RoundRobin,
        };

        let tid = my_thread_create(vehicle_thread, ptr, policy);
        vehicle.thread_id = Some(tid);
    }

    // Esperar a que todos los hilos terminen
    for vehicle in vehicles.iter_mut() {
        if let Some(tid) = vehicle.thread_id {
            my_thread_join(tid);
        }
    }

    println!("\n🏁 Simulación terminada.");
}
