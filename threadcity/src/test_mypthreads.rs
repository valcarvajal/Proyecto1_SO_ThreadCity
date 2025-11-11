// src/test_mypthreads.rs

use mypthreads::*;
use std::os::raw::c_void;
use std::ptr;

/// Estado compartido entre todos los hilos.
#[derive(Debug)]
struct Shared {
    mutex: MyMutex,
    rr_counter: i64,
    lottery_counter: [i64; 3],
    rt_counter: i64,
}

/// Argumentos que pasamos a cada hilo (puntero crudo).
#[repr(C)]
struct WorkerArgs {
    shared: *mut Shared,
    index: usize, // id lógico del hilo dentro de su grupo
}

// ================== Workers ================== //

/// Hilos Round Robin:
/// - Mezclan my_mutex_lock y my_mutex_trylock
/// - Actualizan rr_counter
/// - Hacen yield en cada iteración
extern "C" fn rr_worker(arg: *mut c_void) -> *mut c_void {
    unsafe {
        // Recuperamos y liberamos la caja de argumentos
        let args_box = Box::from_raw(arg as *mut WorkerArgs);
        let shared = args_box.shared;
        let id = args_box.index;

        let mut ok = 0;
        let mut fail = 0;

        for i in 0..2000 {
            // Cada 3 iteraciones hacemos lock bloqueante,
            // las otras veces intentamos con trylock.
            if i % 3 == 0 {
                my_mutex_lock(&mut (*shared).mutex);
                (*shared).rr_counter += 1;
                ok += 1;
                my_mutex_unlock(&mut (*shared).mutex);
            } else {
                let r = my_mutex_trylock(&mut (*shared).mutex);
                if r == 0 {
                    (*shared).rr_counter += 1;
                    ok += 1;
                    my_mutex_unlock(&mut (*shared).mutex);
                } else {
                    fail += 1;
                }
            }

            my_thread_yield();
        }

        println!("[RR] worker {id} terminó: ok={ok}, fail_trylock={fail}");
    }
    ptr::null_mut()
}

/// Hilos Lottery:
/// - Cada uno incrementa su posición en lottery_counter
/// - Todos hacen el mismo número de iteraciones
extern "C" fn lottery_worker(arg: *mut c_void) -> *mut c_void {
    unsafe {
        let args_box = Box::from_raw(arg as *mut WorkerArgs);
        let shared = args_box.shared;
        let idx = args_box.index; // 0, 1, 2

        for _ in 0..4000 {
            my_mutex_lock(&mut (*shared).mutex);
            (*shared).lottery_counter[idx] += 1;
            my_mutex_unlock(&mut (*shared).mutex);

            my_thread_yield();
        }

        println!("[LOT] worker {idx} terminó");
    }
    ptr::null_mut()
}

/// Hilos RealTime:
/// - Incrementan rt_counter
/// - Imprimen ticks
/// - El hilo con index 0 termina explícitamente usando my_thread_end
extern "C" fn rt_worker(arg: *mut c_void) -> *mut c_void {
    unsafe {
        let args_box = Box::from_raw(arg as *mut WorkerArgs);
        let shared = args_box.shared;
        let id = args_box.index;

        for i in 0..80 {
            my_mutex_lock(&mut (*shared).mutex);
            (*shared).rt_counter += 1;
            my_mutex_unlock(&mut (*shared).mutex);

            println!("[RT] tarea {id} tick {i}");

            my_thread_yield();

            // Uno de los hilos RT demuestra uso explícito de my_thread_end
            if id == 0 && i == 40 {
                println!("[RT] tarea {id} finaliza temprano usando my_thread_end");
                my_thread_end(ptr::null_mut());
            }
        }

        println!("[RT] tarea {id} terminó normalmente");
    }
    ptr::null_mut()
}

// ================== main ================== //

fn main() {
    println!("=== Mega test mypthreads ===");

    // Estado compartido
    let mut shared = Shared {
        mutex: MyMutex::new(),
        rr_counter: 0,
        lottery_counter: [0; 3],
        rt_counter: 0,
    };
    my_mutex_init(&mut shared.mutex);

    let shared_ptr = &mut shared as *mut Shared;

    // ----- 1) Hilos Round Robin -----
    let mut rr_ids = Vec::new();
    for i in 0..4 {
        let args = Box::new(WorkerArgs {
            shared: shared_ptr,
            index: i,
        });
        let arg_ptr = Box::into_raw(args) as *mut c_void;

        let tid = my_thread_create(rr_worker, arg_ptr, SchedPolicy::RoundRobin);
        rr_ids.push(tid);
    }

    // Promocionamos el primer RR a Tiempo Real (usa my_thread_chsched)
    let promoted_tid = rr_ids[0];
    let rc = my_thread_chsched(promoted_tid, SchedPolicy::RealTime { deadline: 3 });
    println!("[MAIN] chsched RR->RT para tid {} rc={}", promoted_tid, rc);

    // ----- 2) Hilos Lottery -----
    let mut lot_ids_to_join = Vec::new();
    for i in 0..3 {
        let args = Box::new(WorkerArgs {
            shared: shared_ptr,
            index: i,
        });
        let arg_ptr = Box::into_raw(args) as *mut c_void;

        let tickets = match i {
            0 => 1,
            1 => 3,
            _ => 7,
        };

        let tid = my_thread_create(
            lottery_worker,
            arg_ptr,
            SchedPolicy::Lottery { tickets },
        );

        if i == 2 {
            // Hilo con más tickets -> lo marcamos detached
            let r = my_thread_detach(tid);
            println!(
                "[MAIN] detach de tid {} (lottery con tickets={}) rc={}",
                tid, tickets, r
            );
        } else {
            lot_ids_to_join.push(tid);
        }
    }

    // ----- 3) Hilos RealTime -----
    let mut rt_ids = Vec::new();
    for i in 0..2 {
        let args = Box::new(WorkerArgs {
            shared: shared_ptr,
            index: i,
        });
        let arg_ptr = Box::into_raw(args) as *mut c_void;

        let deadline = if i == 0 { 1 } else { 10 };
        let tid = my_thread_create(
            rt_worker,
            arg_ptr,
            SchedPolicy::RealTime { deadline },
        );
        rt_ids.push(tid);
    }

    // ----- 4) Sincronización con join -----
    // Esperamos a todos los RR (incluyendo el que fue promovido a RealTime)
    for tid in &rr_ids {
        let res = my_thread_join(*tid);
        println!("[MAIN] join RR tid {} -> {:?}", tid, res);
    }

    // Esperamos a los Lottery que NO son detached
    for tid in &lot_ids_to_join {
        let res = my_thread_join(*tid);
        println!("[MAIN] join LOT tid {} -> {:?}", tid, res);
    }

    // Esperamos a las tareas RealTime
    for tid in &rt_ids {
        let res = my_thread_join(*tid);
        println!("[MAIN] join RT tid {} -> {:?}", tid, res);
    }

    // Damos un poco de tiempo extra por si el hilo Lottery detached sigue corriendo
    for _ in 0..1000 {
        my_thread_yield();
    }

    // Intentamos destruir el mutex
    let destroy_rc = my_mutex_destroy(&mut shared.mutex);
    println!("[MAIN] my_mutex_destroy rc={}", destroy_rc);

    // ----- 5) Resumen final -----
    println!("=== Resumen final ===");
    println!("rr_counter        = {}", shared.rr_counter);
    println!("lottery_counters  = {:?}", shared.lottery_counter);
    println!("rt_counter        = {}", shared.rt_counter);
    println!("======================");
}
