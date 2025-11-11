// src/lib.rs

use std::collections::{HashMap, VecDeque};
use std::mem;
use std::os::raw::{c_int, c_void};
use std::ptr;

use libc::{ucontext_t, getcontext, makecontext, swapcontext, EBUSY, EINVAL};

pub type MyThreadId = usize;
pub type ThreadFunc = extern "C" fn(*mut c_void) -> *mut c_void;

/// Estados posibles de un hilo de usuario.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum ThreadState {
    New,
    Ready,
    Running,
    Blocked,
    Finished,
}

/// Políticas de scheduling compatibles.
#[derive(Debug, Copy, Clone)]
pub enum SchedPolicy {
    RoundRobin,
    Lottery { tickets: u32 },
    RealTime { deadline: u64 }, // interpretado como prioridad (menor = más urgente)
}

/// Razón de bloqueo (para depuración/extensión).
#[derive(Debug, Copy, Clone)]
enum BlockReason {
    Join { target: MyThreadId },
    Mutex,
    Other,
}

/// Parámetros de tiempo real (aquí lo mantenemos simple).
#[derive(Debug, Copy, Clone)]
struct RealTimeParams {
    deadline: u64,
}

/// Thread Control Block.
struct Thread {
    id: MyThreadId,
    context: ucontext_t,
    stack: Vec<u8>,
    state: ThreadState,

    scheduler: SchedPolicy,
    tickets: u32,
    rt_params: Option<RealTimeParams>,

    start_routine: Option<ThreadFunc>,
    arg: *mut c_void,
    result: *mut c_void,

    joined_by: Option<MyThreadId>,
    detached: bool,

    block_reason: Option<BlockReason>,
}

/// RNG simple para Lottery scheduler (LCG).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next_u32(&mut self) -> u32 {
        // LCG clásico
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (self.0 >> 32) as u32
    }
}

/// Scheduler global de hilos de usuario.
struct Scheduler {
    threads: HashMap<MyThreadId, Thread>,
    current: Option<MyThreadId>,
    next_id: MyThreadId,

    rr_queue: VecDeque<MyThreadId>,
    lottery_list: Vec<MyThreadId>,
    realtime_list: Vec<MyThreadId>,

    rng: Rng,
}

impl Scheduler {
    fn new() -> Self {
        Scheduler {
            threads: HashMap::new(),
            current: None,
            next_id: 0,
            rr_queue: VecDeque::new(),
            lottery_list: Vec::new(),
            realtime_list: Vec::new(),
            rng: Rng::new(0xdead_beef_cafe_babe),
        }
    }

    /// Inicializa el hilo main (id 0) si aún no existe.
    fn ensure_main_thread(&mut self) {
        if !self.threads.is_empty() {
            return;
        }

        // Capturamos el contexto actual como el hilo 0 (main).
        let mut ctx: ucontext_t = unsafe { mem::zeroed() };
        unsafe {
            getcontext(&mut ctx as *mut ucontext_t);
        }

        let main_thread = Thread {
            id: 0,
            context: ctx,
            stack: Vec::new(), // main usa la pila del proceso
            state: ThreadState::Running,
            scheduler: SchedPolicy::RoundRobin,
            tickets: 0,
            rt_params: None,
            start_routine: None,
            arg: ptr::null_mut(),
            result: ptr::null_mut(),
            joined_by: None,
            detached: false,
            block_reason: None,
        };

        self.threads.insert(0, main_thread);
        self.current = Some(0);
        self.next_id = 1;
    }

    fn current_thread_id(&self) -> Option<MyThreadId> {
        self.current
    }

    fn get_thread(&self, id: MyThreadId) -> Option<&Thread> {
        self.threads.get(&id)
    }

    fn get_thread_mut(&mut self, id: MyThreadId) -> Option<&mut Thread> {
        self.threads.get_mut(&id)
    }

    /// Inserta un hilo en la cola de Ready correspondiente, según su política.
    fn enqueue_ready(&mut self, tid: MyThreadId) {
        let t = self.threads.get(&tid).expect("thread no encontrado en enqueue_ready");
        match t.scheduler {
            SchedPolicy::RoundRobin => self.rr_queue.push_back(tid),
            SchedPolicy::Lottery { .. } => self.lottery_list.push(tid),
            SchedPolicy::RealTime { .. } => self.realtime_list.push(tid),
        }
    }

    /// Elimina un hilo de todas las estructuras de Ready (por cambio de scheduler, bloqueo, etc.).
    fn remove_from_ready_lists(&mut self, tid: MyThreadId) {
        self.rr_queue.retain(|&id| id != tid);
        self.lottery_list.retain(|&id| id != tid);
        self.realtime_list.retain(|&id| id != tid);
    }

    /// Crea un nuevo hilo y lo deja en estado Ready.
    fn create_thread(
        &mut self,
        start_routine: ThreadFunc,
        arg: *mut c_void,
        policy: SchedPolicy,
    ) -> MyThreadId {
        self.ensure_main_thread();

        let id = self.next_id;
        self.next_id += 1;

        const STACK_SIZE: usize = 64 * 1024; // 64 KB (ajustable)
        let mut stack = vec![0u8; STACK_SIZE];

        let mut ctx: ucontext_t = unsafe { mem::zeroed() };
        unsafe {
            getcontext(&mut ctx as *mut ucontext_t);

            // Asociar la pila al contexto
            ctx.uc_stack.ss_sp = stack.as_mut_ptr() as *mut c_void;
            ctx.uc_stack.ss_size = STACK_SIZE;
            ctx.uc_link = ptr::null_mut();

            // thread_trampoline no recibe argumentos en este diseño.
            makecontext(
                &mut ctx as *mut ucontext_t,
                thread_trampoline as extern "C" fn(),
                0,
            );
        }

        // Configurar tickets / RT params según la política
        let mut tickets = 0;
        let mut rt_params = None;

        match policy {
            SchedPolicy::RoundRobin => {}
            SchedPolicy::Lottery { tickets: t } => {
                tickets = if t == 0 { 1 } else { t };
            }
            SchedPolicy::RealTime { deadline } => {
                rt_params = Some(RealTimeParams { deadline });
            }
        }

        let t = Thread {
            id,
            context: ctx,
            stack,
            state: ThreadState::Ready,
            scheduler: policy,
            tickets,
            rt_params,
            start_routine: Some(start_routine),
            arg,
            result: ptr::null_mut(),
            joined_by: None,
            detached: false,
            block_reason: None,
        };

        self.threads.insert(id, t);
        self.enqueue_ready(id);

        id
    }

    /// Selecciona el próximo hilo a ejecutar según RT > Lottery > RR.
    fn pick_next(&mut self) -> Option<MyThreadId> {
        // Hilos de Tiempo Real: menor deadline primero
        if !self.realtime_list.is_empty() {
            let mut best_idx = 0;
            let mut best_deadline = {
                let tid = self.realtime_list[0];
                let t = self.threads.get(&tid).unwrap();
                t.rt_params.unwrap().deadline
            };

            for (i, &tid) in self.realtime_list.iter().enumerate().skip(1) {
                let d = self.threads.get(&tid).unwrap().rt_params.unwrap().deadline;
                if d < best_deadline {
                    best_deadline = d;
                    best_idx = i;
                }
            }

            let tid = self.realtime_list.remove(best_idx);
            let thr = self.threads.get_mut(&tid).unwrap();
            thr.state = ThreadState::Running;
            return Some(tid);
        }

        // Lottery scheduler
        if !self.lottery_list.is_empty() {
            let total_tickets: u32 = self
                .lottery_list
                .iter()
                .map(|tid| self.threads.get(tid).unwrap().tickets)
                .sum();

            if total_tickets > 0 {
                let mut r = self.rng.next_u32() % total_tickets;
                let mut winner_idx = 0;

                for (i, &tid) in self.lottery_list.iter().enumerate() {
                    let t = self.threads.get(&tid).unwrap().tickets;
                    if r < t {
                        winner_idx = i;
                        break;
                    } else {
                        r -= t;
                    }
                }

                let tid = self.lottery_list.remove(winner_idx);
                let thr = self.threads.get_mut(&tid).unwrap();
                thr.state = ThreadState::Running;
                return Some(tid);
            }
        }

        // Round Robin
        if let Some(tid) = self.rr_queue.pop_front() {
            let thr = self.threads.get_mut(&tid).unwrap();
            thr.state = ThreadState::Running;
            return Some(tid);
        }

        None
    }

    /// El hilo actual cede la CPU voluntariamente.
    fn yield_current(&mut self) {
        self.ensure_main_thread();

        let curr_id = match self.current {
            Some(id) => id,
            None => return,
        };

        // Marcar actual como Ready y encolar
        {
            let thr = self.threads.get_mut(&curr_id).unwrap();
            if thr.state == ThreadState::Running {
                thr.state = ThreadState::Ready;
                self.enqueue_ready(curr_id);
            }
        }

        // Elegir siguiente
        if let Some(next_id) = self.pick_next() {
            if next_id == curr_id {
                return;
            }

            // Preparar punteros a contextos
            let (curr_ctx_ptr, next_ctx_ptr) = {
                let curr_ctx: *mut ucontext_t =
                    &mut self.threads.get_mut(&curr_id).unwrap().context;
                let next_ctx: *mut ucontext_t =
                    &mut self.threads.get_mut(&next_id).unwrap().context;
                (curr_ctx, next_ctx)
            };

            self.current = Some(next_id);

            unsafe {
                swapcontext(curr_ctx_ptr, next_ctx_ptr);
            }
        }
    }

    /// Bloquea el hilo actual (por mutex, join, etc.) y hace schedule.
    fn block_current(&mut self, reason: BlockReason) {
        self.ensure_main_thread();

        let curr_id = self.current.expect("no hay hilo actual en block_current");

        {
            let thr = self.threads.get_mut(&curr_id).unwrap();
            thr.state = ThreadState::Blocked;
            thr.block_reason = Some(reason);
        }

        self.remove_from_ready_lists(curr_id);

        // Elegir siguiente
        if let Some(next_id) = self.pick_next() {
            let (curr_ctx_ptr, next_ctx_ptr) = {
                let curr_ctx: *mut ucontext_t =
                    &mut self.threads.get_mut(&curr_id).unwrap().context;
                let next_ctx: *mut ucontext_t =
                    &mut self.threads.get_mut(&next_id).unwrap().context;
                (curr_ctx, next_ctx)
            };
            self.current = Some(next_id);

            unsafe {
                swapcontext(curr_ctx_ptr, next_ctx_ptr);
            }
        } else {
            // No hay nadie más: deadlock o todos bloqueados.
            // En un sistema real habría que manejar esto mejor.
        }
    }

    /// Marca un hilo como Ready y lo encola en su scheduler.
    fn unblock(&mut self, tid: MyThreadId) {
        if let Some(thr) = self.threads.get_mut(&tid) {
            thr.state = ThreadState::Ready;
            thr.block_reason = None;
            self.enqueue_ready(tid);
        }
    }

    /// Finaliza el hilo actual y pasa a otro.
    fn finish_current(&mut self, retval: *mut c_void) -> ! {
        self.ensure_main_thread();

        let curr_id = self.current.expect("no hay hilo actual en finish_current");

        let joined_by = {
            let thr = self.threads.get_mut(&curr_id).unwrap();
            thr.state = ThreadState::Finished;
            thr.result = retval;
            thr.joined_by
        };

        // Despertar al que hizo join, si existe
        if let Some(jid) = joined_by {
            self.unblock(jid);
        }

        // No lo encolamos de nuevo (ya terminó)
        self.remove_from_ready_lists(curr_id);

        // Elegir siguiente
        if let Some(next_id) = self.pick_next() {
            let curr_ctx_ptr: *mut ucontext_t =
                &mut self.threads.get_mut(&curr_id).unwrap().context;
            let next_ctx_ptr: *mut ucontext_t =
                &mut self.threads.get_mut(&next_id).unwrap().context;

            self.current = Some(next_id);

            unsafe {
                swapcontext(curr_ctx_ptr, next_ctx_ptr);
            }

            // Si volvemos aquí es que algo salió muy raro
            unsafe { core::hint::unreachable_unchecked() }
        } else {
            // No hay más hilos ready: podemos volver a main si se manejara aparte,
            // o terminar el proceso.
            std::process::exit(0);
        }
    }

    /// Intenta hacer join inmediato; si el hilo ya terminó, retorna Some(result).
    fn try_join_immediate(&self, target: MyThreadId) -> Option<*mut c_void> {
        let t = self.threads.get(&target)?;
        if t.state == ThreadState::Finished {
            Some(t.result)
        } else {
            None
        }
    }

    /// Cambia la política de scheduling de un hilo.
    fn change_scheduler(&mut self, tid: MyThreadId, policy: SchedPolicy) -> c_int {
        if !self.threads.contains_key(&tid) {
            return EINVAL;
        }

        self.remove_from_ready_lists(tid);

        {
            let thr = self.threads.get_mut(&tid).unwrap();
            thr.scheduler = policy;
            thr.tickets = 0;
            thr.rt_params = None;

            match policy {
                SchedPolicy::RoundRobin => {}
                SchedPolicy::Lottery { tickets } => {
                    thr.tickets = if tickets == 0 { 1 } else { tickets };
                }
                SchedPolicy::RealTime { deadline } => {
                    thr.rt_params = Some(RealTimeParams { deadline });
                }
            }
        }

        // Si el hilo está listo, re-encolar según nueva política.
        if self.threads.get(&tid).unwrap().state == ThreadState::Ready {
            self.enqueue_ready(tid);
        }

        0
    }
}

/// Scheduler global en espacio de usuario.
static mut SCHEDULER: *mut Scheduler = std::ptr::null_mut();

/// Acceso global al scheduler (lazy-init).
fn scheduler() -> &'static mut Scheduler {
    unsafe {
        if SCHEDULER.is_null() {
            let boxed = Box::new(Scheduler::new());
            let leaked: &'static mut Scheduler = Box::leak(boxed);
            SCHEDULER = leaked as *mut Scheduler;
        }
        &mut *SCHEDULER
    }
}

/// Trampolín: es la función que todos los hilos nuevos ejecutan primero.
extern "C" fn thread_trampoline() {
    unsafe {
        let sched = scheduler();
        let tid = sched.current_thread_id().expect("no current thread in trampoline");

        // Obtenemos función y argumento del TCB
        let (func, arg) = {
            let t = sched.get_thread(tid).expect("thread not found in trampoline");
            (t.start_routine.expect("no start_routine"), t.arg)
        };

        let result = func(arg);
        my_thread_end(result);
    }
}

// ============ API pública estilo mypthreads ============ //

/// Crea un hilo de usuario con la política indicada.
/// Devuelve el id del hilo (MyThreadId).
pub fn my_thread_create(
    start_routine: ThreadFunc,
    arg: *mut c_void,
    policy: SchedPolicy,
) -> MyThreadId {
    unsafe { scheduler().create_thread(start_routine, arg, policy) }
}

/// Finaliza el hilo actual, devolviendo `retval` a quien haga join.
/// No debería regresar.
pub fn my_thread_end(retval: *mut c_void) -> ! {
    unsafe { scheduler().finish_current(retval) }
}

/// El hilo actual cede la CPU.
pub fn my_thread_yield() {
    unsafe {
        scheduler().yield_current();
    }
}

/// Bloquea hasta que el hilo `target` termine y devuelve su resultado.
pub fn my_thread_join(target: MyThreadId) -> *mut c_void {
    unsafe {
        let sched = scheduler();
        let curr = sched.current_thread_id().expect("join sin hilo actual");

        if curr == target {
            // No tiene sentido hacer join a uno mismo.
            return ptr::null_mut();
        }

        if let Some(res) = sched.try_join_immediate(target) {
            return res;
        }

        // Bloqueamos el hilo actual en espera del target
        {
            let t = sched.get_thread_mut(target).expect("target de join no encontrado");
            t.joined_by = Some(curr);
        }

        scheduler().block_current(BlockReason::Join { target });

        // Cuando despertamos, ya terminó
        let res = scheduler()
            .get_thread(target)
            .expect("thread desapareció durante join")
            .result;
        res
    }
}

/// Marca un hilo como detached (no se espera join).
pub fn my_thread_detach(tid: MyThreadId) -> c_int {
    unsafe {
        let sched = scheduler();
        if let Some(t) = sched.get_thread_mut(tid) {
            t.detached = true;
            0
        } else {
            EINVAL
        }
    }
}

/// Cambia la política de scheduling de un hilo.
pub fn my_thread_chsched(tid: MyThreadId, policy: SchedPolicy) -> c_int {
    unsafe { scheduler().change_scheduler(tid, policy) }
}

// ============ Implementación del mutex propio (mymutex) ============ //

#[derive(Debug)]
pub struct MyMutex {
    locked: bool,
    owner: Option<MyThreadId>,
    waiters: VecDeque<MyThreadId>,
}

impl MyMutex {
    pub fn new() -> Self {
        MyMutex {
            locked: false,
            owner: None,
            waiters: VecDeque::new(),
        }
    }
}

/// Inicializa un mutex.
pub fn my_mutex_init(m: &mut MyMutex) -> c_int {
    *m = MyMutex::new();
    0
}

/// Destruye un mutex (simple, sin liberar recursos extra).
pub fn my_mutex_destroy(m: &mut MyMutex) -> c_int {
    if m.locked || !m.waiters.is_empty() {
        // Semántica aproximada a pthread: no destruir si está bloqueado.
        EBUSY
    } else {
        // Nada especial que hacer.
        0
    }
}

/// Intenta tomar el lock; si está ocupado, retorna EBUSY.
pub fn my_mutex_trylock(m: &mut MyMutex) -> c_int {
    unsafe {
        let sched = scheduler();
        let curr = sched.current_thread_id().expect("trylock sin hilo actual");

        if !m.locked {
            m.locked = true;
            m.owner = Some(curr);
            0
        } else {
            EBUSY
        }
    }
}

/// Bloquea hasta adquirir el mutex.
pub fn my_mutex_lock(m: &mut MyMutex) -> c_int {
    unsafe {
        let sched = scheduler();
        let curr = sched.current_thread_id().expect("lock sin hilo actual");

        if !m.locked {
            m.locked = true;
            m.owner = Some(curr);
            return 0;
        }

        // Si ya está tomado, nos encolamos y bloqueamos
        m.waiters.push_back(curr);
        scheduler().block_current(BlockReason::Mutex);

        // Cuando el hilo despierte, debe ser el dueño del mutex
        debug_assert!(m.locked);
        debug_assert_eq!(m.owner, Some(curr));

        0
    }
}

/// Libera el mutex y despierta a un waiter si existe.
pub fn my_mutex_unlock(m: &mut MyMutex) -> c_int {
    unsafe {
        let sched = scheduler();
        let curr = sched.current_thread_id().expect("unlock sin hilo actual");

        if m.owner != Some(curr) {
            // No es el dueño del mutex
            return EINVAL;
        }

        if let Some(next_tid) = m.waiters.pop_front() {
            // Le pasamos el lock directamente al siguiente hilo
            m.locked = true;
            m.owner = Some(next_tid);
            scheduler().unblock(next_tid);
        } else {
            // No hay nadie esperando
            m.locked = false;
            m.owner = None;
        }

        0
    }
}
