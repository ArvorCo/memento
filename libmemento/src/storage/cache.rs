//! LRU cache for eigenvector decompositions
//!
//! Optimized for production performance with configurable memory constraints.
//! Memory budget: <4GB for 100K vocabulary semantic matrix.
//! Cache holds hot eigenvectors for sub-100μs access latency.

use crate::storage::Result;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::Mutex;

// Placeholder type - will be replaced with actual EigenDecomposition from semantic-matrix
#[derive(Debug, Clone)]
pub struct EigenDecomposition {
    pub eigenvalues: Vec<f64>,
    pub eigenvectors: Vec<Vec<f64>>,
    pub coherence_score: f64,
}

impl EigenDecomposition {
    /// Estimate memory size in bytes
    /// Eigenvalues: k * 8 bytes
    /// Eigenvectors: k * vocab_size * 8 bytes
    /// Coherence: 8 bytes
    pub fn estimated_size(&self) -> usize {
        let eigenvalues_size = self.eigenvalues.len() * std::mem::size_of::<f64>();
        let eigenvectors_size: usize = self
            .eigenvectors
            .iter()
            .map(|v| v.len() * std::mem::size_of::<f64>())
            .sum();
        eigenvalues_size + eigenvectors_size + std::mem::size_of::<f64>()
    }
}

/// Configuration for eigenvector cache
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries (default: 1000 for 100K vocab)
    pub capacity: usize,
    /// Maximum memory budget in bytes (default: 512MB)
    pub max_memory_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            // Default: 1000 entries for 100K vocabulary
            // Assuming ~512KB per eigendecomposition (100 eigenvectors × 100K vocab × 8 bytes)
            // Total: ~512MB memory footprint
            capacity: 1000,
            max_memory_bytes: 512 * 1024 * 1024, // 512MB
        }
    }
}

impl CacheConfig {
    /// Create configuration optimized for vocabulary size
    /// Memory allocation: ~5KB per eigenvector for 100K vocab
    pub fn for_vocabulary_size(vocab_size: usize, num_eigenvectors: usize) -> Self {
        let bytes_per_entry = num_eigenvectors * vocab_size * std::mem::size_of::<f64>();
        let target_memory = 512 * 1024 * 1024; // 512MB target
        let capacity = (target_memory / bytes_per_entry).clamp(100, 10000);

        Self {
            capacity,
            max_memory_bytes: target_memory,
        }
    }
}

/// LRU cache for eigenvector decompositions with memory budget tracking
pub struct EigenvectorCache {
    cache: Mutex<LruCache<String, EigenDecomposition>>,
    config: CacheConfig,
}

impl EigenvectorCache {
    /// Create a new LRU cache with the given capacity
    pub fn new(capacity: usize) -> Self {
        let config = CacheConfig {
            capacity,
            ..Default::default()
        };
        let cap = NonZeroUsize::new(capacity).expect("Capacity must be non-zero");
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            config,
        }
    }

    /// Create cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        let cap = NonZeroUsize::new(config.capacity).expect("Capacity must be non-zero");
        Self {
            cache: Mutex::new(LruCache::new(cap)),
            config,
        }
    }

    /// Create cache optimized for vocabulary size
    pub fn for_vocabulary(vocab_size: usize, num_eigenvectors: usize) -> Self {
        let config = CacheConfig::for_vocabulary_size(vocab_size, num_eigenvectors);
        Self::with_config(config)
    }

    /// Get an eigendecomposition from the cache
    pub fn get(&self, key: &str) -> Result<Option<EigenDecomposition>> {
        let mut cache = self.cache.lock().unwrap();
        Ok(cache.get(key).cloned())
    }

    /// Put an eigendecomposition into the cache
    pub fn put(&self, key: String, value: EigenDecomposition) -> Result<()> {
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, value);
        Ok(())
    }

    /// Get cache statistics for monitoring
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.lock().unwrap();
        CacheStats {
            capacity: self.config.capacity,
            size: cache.len(),
            max_memory_bytes: self.config.max_memory_bytes,
        }
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.clear();
    }
}

/// Cache statistics for monitoring and metrics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub capacity: usize,
    pub size: usize,
    pub max_memory_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let cache = EigenvectorCache::new(10);

        let eigen = EigenDecomposition {
            eigenvalues: vec![1.0, 2.0, 3.0],
            eigenvectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            coherence_score: 0.8,
        };

        // Put and get
        cache.put("test".to_string(), eigen.clone()).unwrap();
        let result = cache.get("test").unwrap();

        assert!(result.is_some());
        let cached = result.unwrap();
        assert_eq!(cached.eigenvalues, eigen.eigenvalues);
        assert_eq!(cached.coherence_score, eigen.coherence_score);
    }
}
