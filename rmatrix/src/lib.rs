//! RMatrix - Una librería para manejo de matrices en Rust
//!
//! # Ejemplos
//! ```
//! use rmatrix::Matrix;
//!
//! let mut mat = Matrix::<i32>::new(2, 3);
//! mat.set(0, 1, 42);
//! assert_eq!(*mat.get(0, 1), 42);
//! ```

use num_traits::{Zero, One};

/// Representa una matriz de elementos genéricos
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix<T> {
    data: Vec<T>,
    rows: usize,
    cols: usize,
}

impl<T> Matrix<T> {
    /// Crea una nueva matriz con las dimensiones especificadas
    ///
    /// # Argumentos
    /// * `rows` - Número de filas
    /// * `cols` - Número de columnas
    ///
    /// # Ejemplos
    /// ```
    /// use rmatrix::Matrix;
    ///
    /// let mat = Matrix::<i32>::new(3, 4);
    /// ```
    pub fn new(rows: usize, cols: usize) -> Self
    where
        T: Default + Clone,
    {
        Matrix {
            data: vec![T::default(); rows * cols],
            rows,
            cols,
        }
    }

    /// Obtiene una referencia al elemento en la posición (row, col)
    ///
    /// # Argumentos
    /// * `row` - Índice de la fila (0-based)
    /// * `col` - Índice de la columna (0-based)
    ///
    /// # Panics
    /// Panics si los índices están fuera de los límites
    pub fn get(&self, row: usize, col: usize) -> &T {
        &self.data[row * self.cols + col]
    }

    /// Obtiene una referencia mutable al elemento en la posición (row, col)
    ///
    /// # Argumentos
    /// * `row` - Índice de la fila (0-based)
    /// * `col` - Índice de la columna (0-based)
    ///
    /// # Panics
    /// Panics si los índices están fuera de los límites
    pub fn get_mut(&mut self, row: usize, col: usize) -> &mut T {
        &mut self.data[row * self.cols + col]
    }

    /// Establece el valor en la posición (row, col)
    ///
    /// # Argumentos
    /// * `row` - Índice de la fila (0-based)
    /// * `col` - Índice de la columna (0-based)
    /// * `value` - Valor a establecer
    ///
    /// # Panics
    /// Panics si los índices están fuera de los límites
    pub fn set(&mut self, row: usize, col: usize, value: T) {
        self.data[row * self.cols + col] = value;
    }

    /// Devuelve el número de filas de la matriz
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Devuelve el número de columnas de la matriz
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Devuelve las dimensiones de la matriz como (filas, columnas)
    pub fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    /// Crea una matriz a partir de un vector y dimensiones
    ///
    /// # Argumentos
    /// * `data` - Vector con los datos en orden de filas
    /// * `rows` - Número de filas
    /// * `cols` - Número de columnas
    ///
    /// # Panics
    /// Panics si la longitud del vector no coincide con rows * cols
    pub fn from_vec(data: Vec<T>, rows: usize, cols: usize) -> Self {
        if data.len() != rows * cols {
            panic!("La longitud del vector debe ser rows * cols");
        }
        Matrix { data, rows, cols }
    }

    /// Devuelve una referencia al vector de datos subyacente
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// Devuelve una referencia mutable al vector de datos subyacente
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
}

// Implementación para tipos que pueden ser inicializados a cero
impl<T> Matrix<T>
where
    T: Default + Clone,
{
    /// Crea una matriz llena de ceros
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self::new(rows, cols)
    }
}

// Implementación para tipos numéricos con identidad (Zero y One)
impl<T> Matrix<T>
where
    T: Default + Clone + Zero + One,
{
    /// Crea una matriz identidad (solo para matrices cuadradas)
    pub fn identity(size: usize) -> Self {
        let mut mat = Self::zeros(size, size);
        for i in 0..size {
            mat.set(i, i, T::one());
        }
        mat
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matrix_creation() {
        let mat = Matrix::<i32>::new(2, 3);
        assert_eq!(mat.rows(), 2);
        assert_eq!(mat.cols(), 3);
        assert_eq!(mat.dimensions(), (2, 3));
    }

    #[test]
    fn test_get_set() {
        let mut mat = Matrix::<i32>::new(2, 2);
        mat.set(0, 0, 1);
        mat.set(0, 1, 2);
        mat.set(1, 0, 3);
        mat.set(1, 1, 4);

        assert_eq!(*mat.get(0, 0), 1);
        assert_eq!(*mat.get(0, 1), 2);
        assert_eq!(*mat.get(1, 0), 3);
        assert_eq!(*mat.get(1, 1), 4);
    }

    #[test]
    fn test_get_mut() {
        let mut mat = Matrix::<i32>::new(2, 2);
        *mat.get_mut(0, 0) = 10;
        *mat.get_mut(1, 1) = 20;

        assert_eq!(*mat.get(0, 0), 10);
        assert_eq!(*mat.get(1, 1), 20);
    }

    #[test]
    fn test_from_vec() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let mat = Matrix::from_vec(data, 2, 3);

        assert_eq!(mat.rows(), 2);
        assert_eq!(mat.cols(), 3);
        assert_eq!(*mat.get(0, 0), 1);
        assert_eq!(*mat.get(1, 2), 6);
    }

    #[test]
    fn test_as_slice() {
        let mut mat = Matrix::<i32>::new(2, 2);
        mat.set(0, 0, 1);
        mat.set(0, 1, 2);
        mat.set(1, 0, 3);
        mat.set(1, 1, 4);

        let slice = mat.as_slice();
        assert_eq!(slice, &[1, 2, 3, 4]);
    }

    #[test]
    fn test_identity() {
        let mat = Matrix::<i32>::identity(3);
        assert_eq!(*mat.get(0, 0), 1);
        assert_eq!(*mat.get(1, 1), 1);
        assert_eq!(*mat.get(2, 2), 1);
        assert_eq!(*mat.get(0, 1), 0);
        assert_eq!(*mat.get(1, 0), 0);
    }
}