use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use mypthreads::*;
use rmatrix::*;

static SIM_TIME: AtomicU64 = AtomicU64::new(0);

/// Tiempo de simulación actual (en ticks lógicos).
pub fn sim_time_now() -> u64 {
    SIM_TIME.load(Ordering::Relaxed)
}

/// Avanza el tiempo de simulación en `delta` ticks.
pub fn sim_time_advance(delta: u64) {
    SIM_TIME.fetch_add(delta, Ordering::Relaxed);
}

/// Ancho y altura de la grid de la ciudad.
pub const GRID_WIDTH: usize = 16;
pub const GRID_HEIGHT: usize = 20;

/// Número mínimo y máximo de carros en la simulación.
const MIN_CARS: usize = 23;
const MAX_CARS: usize = 27;

/// Contador atómico de carros activos en la simulación.
static ACTIVE_CARS: AtomicUsize = AtomicUsize::new(0);

fn active_cars() -> usize {
    ACTIVE_CARS.load(Ordering::Relaxed)
}

fn inc_cars() {
    ACTIVE_CARS.fetch_add(1, Ordering::Relaxed);
}

fn dec_cars() {
    ACTIVE_CARS.fetch_sub(1, Ordering::Relaxed);
}

/// Coordenada (x, y) en la grid: x = columna, y = fila.
pub type Coord = (usize, usize);

/// ID lógico de vehículo dentro de la simulación.
pub type VehicleId = usize;

/// Tipo de bloque en el grid.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]  // Agregado Hash
pub enum BlockKind {
    Path,                      // carreteras y puentes
    Building,                  // construcciones
    River,                     // río (prohibido para carros)
    Shop,                      // tiendas
    NuclearPlant,              // parte de una planta nuclear
    Hospital,                  // parte de un hospital
    Dock,                      // atracadero
}

/// Tarea de bloque en el grid.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]  // Agregado Hash
pub enum BlockTask {
    None,                      // ninguna
    Spawn,                     // punto de salida
    Finish,                    // punto de llegada
}

/// Bloque de la ciudad
#[derive(Debug)]
pub struct Block {
    pub kind: BlockKind,
    pub task: BlockTask,
    pub occupant: Option<VehicleId>,
    pub lock: MyMutex,
}

impl Default for Block {
    fn default() -> Self {
        Block {
            kind: BlockKind::Path,
            task: BlockTask::None,
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
            occupant: self.occupant,
            lock: MyMutex::new(),  // Los mutex no se clonan, creamos uno nuevo
        }
    }
}

/// Tipos de vehículo de la simulación.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VehicleKind {
    Car,               // carro normal
    Ambulance,         // ambulancia
    TruckWater,        // camión de agua
    TruckRadioactive,  // camión de material radiactivo
    Boat,              // barco
}

/// Estado/metadata de un vehículo.
#[derive(Debug)]
pub struct Vehicle {
    pub id: VehicleId,
    pub kind: VehicleKind,
    pub pos: Coord,
    pub dest: Coord,
    pub thread_id: Option<MyThreadId>,   // hilo mypthread que lo controla
    pub base_policy: SchedPolicy,        // scheduler "natural" del vehículo
}

/// Crea una ciudad detallada con el patrón especificado
pub fn create_detailed_city() -> Matrix<Block> {
    let mut city = Matrix::<Block>::new(GRID_HEIGHT, GRID_WIDTH);
    
    // Patrón detallado basado en tu especificación
    let pattern: [[char; GRID_WIDTH]; GRID_HEIGHT] = [
        // Fila 0
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 1
        ['p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p'],
        // Fila 2
        ['p', 'b', 'b', 'p', 'b', 's', 'p', 'b', 'b', 'p', 's', 'b', 'p', 'b', 'b', 'p'],
        // Fila 3
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 4
        ['p', 'b', 'b', 'p', 'n', 'n', 'p', 'b', 's', 'p', 'h', 'h', 'p', 'b', 'b', 'p'],
        // Fila 5
        ['p', 'b', 'b', 'p', 'n', 'n', 'p', 's', 'b', 'p', 'h', 'h', 'p', 'b', 'b', 'p'],
        // Fila 6
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 7
        ['p', 'b', 'b', 'p', 'b', 's', 'p', 'b', 'b', 'p', 's', 'b', 'p', 'b', 'b', 'p'],
        // Fila 8
        ['p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p'],
        // Fila 9
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 10
        ['r', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'r'],
        // Fila 11
        ['r', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'r', 'r'],
        // Fila 12
        ['r', 'r', 'r', 'p', 'r', 'r', 'p', 'r', 'd', 'p', 'r', 'r', 'p', 'r', 'r', 'r'],
        // Fila 13
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 14
        ['p', 'b', 'b', 'p', 'b', 'b', 'p', 'n', 'n', 'p', 'b', 'b', 'p', 'b', 'b', 'p'],
        // Fila 15
        ['p', 'b', 'b', 'p', 's', 'b', 'p', 'n', 'n', 'p', 'b', 's', 'p', 'b', 'b', 'p'],
        // Fila 16
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
        // Fila 17
        ['p', 'b', 'b', 'p', 'b', 's', 'p', 'b', 'b', 'p', 's', 'b', 'p', 'b', 'b', 'p'],
        // Fila 18
        ['p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p', 'b', 'b', 'p'],
        // Fila 19
        ['p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p', 'p'],
    ];
    
    // Inicializar la ciudad según el patrón detallado
    for row in 0..GRID_HEIGHT {
        for col in 0..GRID_WIDTH {
            let (block_kind, block_task) = match pattern[row][col] {
                'p' => (BlockKind::Path, BlockTask::None),
                'b' => (BlockKind::Building, BlockTask::None),
                'r' => (BlockKind::River, BlockTask::None),
                's' => (BlockKind::Shop, BlockTask::Finish),      // Shop = Finish
                'n' => (BlockKind::NuclearPlant, BlockTask::Finish), // NuclearPlant = Finish
                'h' => (BlockKind::Hospital, BlockTask::Finish),  // Hospital = Finish
                'd' => (BlockKind::Dock, BlockTask::Finish),      // Dock = Finish
                _ => (BlockKind::Path, BlockTask::None),          // Por defecto
            };
            
            city.set(row, col, Block {
                kind: block_kind,
                task: block_task,
                occupant: None,
                lock: MyMutex::new(),
            });
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
            
            // Mostrar si tiene task Finish
            if block.task == BlockTask::Spawn {
                print!("◉ ");
            } else {
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

/// Función para contar bloques por tarea
pub fn count_blocks_by_task(city: &Matrix<Block>) -> HashMap<BlockTask, usize> {
    let mut counter = HashMap::new();
    
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let task = city.get(row, col).task;
            *counter.entry(task).or_insert(0) += 1;
        }
    }
    
    counter
}

/// Encuentra todas las posiciones de destino (Finish) en la ciudad
pub fn find_finish_positions(city: &Matrix<Block>) -> Vec<Coord> {
    let mut positions = Vec::new();
    
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            if city.get(row, col).task == BlockTask::Finish {
                positions.push((row, col));
            }
        }
    }
    
    positions
}

/// Encuentra posiciones de spawn (podrías agregar algunas después)
pub fn find_spawn_positions(city: &Matrix<Block>) -> Vec<Coord> {
    let mut positions = Vec::new();
    
    // Buscar en los bordes de Path para spawn points
    for row in 0..city.rows() {
        for col in 0..city.cols() {
            let block = city.get(row, col);
            if block.kind == BlockKind::Path && block.task == BlockTask::Spawn {
                positions.push((row, col));
            }
        }
    }
    
    positions
}

/// Configura algunos puntos de spawn en la ciudad
pub fn setup_spawn_points(city: &mut Matrix<Block>) {
    // Puntos de spawn en los bordes de la ciudad (solo en Path)
    let spawn_candidates = [
        (0, 0), (0, 6), (0, 9), (0, 15),               // Borde superior
        (19, 0), (19, 6), (19, 9), (19, 15),           // Borde inferior
        (3, 0), (6, 0), (9, 0), (13, 0), (16, 0),      // Borde izquierdo
        (3, 15), (6, 15), (9, 15), (13, 15), (16, 15)  // Borde derecho
    ];
    
    for &(row, col) in &spawn_candidates {
        if row < city.rows() && col < city.cols() {
            // CORRECCIÓN: get_mut no devuelve Option, devuelve &mut Block directamente
            let block = city.get_mut(row, col);
            if block.kind == BlockKind::Path {
                block.task = BlockTask::Spawn;
            }
        }
    }
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

fn main() {
    // Crear la ciudad detallada
    let mut city = create_detailed_city();
    
    // Configurar puntos de spawn
    setup_spawn_points(&mut city);
    
    // Mostrar la ciudad
    print_detailed_city(&city);
    
    // Mostrar estadísticas
    let kind_stats = count_blocks_by_kind(&city);
    let task_stats = count_blocks_by_task(&city);
    let finish_positions = find_finish_positions(&city);
    let spawn_positions = find_spawn_positions(&city);
    
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
    
    println!("\nPor tarea:");
    for (task, count) in task_stats {
        let task_name = match task {
            BlockTask::None => "None",
            BlockTask::Spawn => "Spawn",
            BlockTask::Finish => "Finish",
        };
        println!("  {}: {}", task_name, count);
    }
    
    println!("\nPosiciones de destino (Finish): {}", finish_positions.len());
    println!("Posiciones de spawn: {}", spawn_positions.len());
    
    // Ejemplo de uso para validación de posiciones
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
        ].iter() {
            let is_valid = is_valid_position_for_vehicle(&city, pos, *vehicle_kind);
            println!("  {:?}: {}", vehicle_kind, is_valid);
        }
    }
}