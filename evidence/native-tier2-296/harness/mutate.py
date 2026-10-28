import os
import sys
cell = sys.argv[1]
base_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'base')
base_pool = os.path.join(base_dir, 'base-pool.rs')
base_lib = os.path.join(base_dir, 'base-lib.rs')
pool = 'crates/qianqian-diagnostics/src/pool.rs'
lib = 'crates/qianqian-diagnostics/src/lib.rs'
def patch(path, old, new):
    s = open(path).read()
    assert old in s, (path, cell)
    open(path, 'w').write(s.replace(old, new, 1))
if cell == 'C1':
    open(pool,'w').write(open(base_pool).read())
    patch(pool, '''        let p = self.pub_gen.load(Ordering::Acquire);
        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return false;
        }
''', '''        let p = self.pub_gen.load(Ordering::Acquire);
        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            // QFB-MUTANT-C1: break producer isolation by making the producer
            // wait (bounded) for the blocked consumer to free a slot.
            let start = std::time::Instant::now();
            while self
                .pub_gen
                .load(Ordering::Acquire)
                .wrapping_sub(self.cons_gen.load(Ordering::Acquire))
                == 2
                && start.elapsed() < std::time::Duration::from_secs(5)
            {
                std::thread::yield_now();
            }
            if self
                .pub_gen
                .load(Ordering::Acquire)
                .wrapping_sub(self.cons_gen.load(Ordering::Acquire))
                == 2
            {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return false;
            }
        }
''')
elif cell == 'C2':
    open(lib,'w').write(open(base_lib).read())
    patch(lib, '''                return ShutdownReport {
                    drained: false,
                    abandoned: true,''', '''                return ShutdownReport {
                    // QFB-MUTANT-C2: claim a complete drain while the drainer
                    // is still stuck in the exporter.
                    drained: true,
                    abandoned: false,''')
elif cell == 'C3':
    open(pool,'w').write(open(base_pool).read())
    patch(pool, '''        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return false;
        }
''', '''        if p.wrapping_sub(self.cons_gen.load(Ordering::Acquire)) == 2 {
            // QFB-MUTANT-C3: remove the pool's only loss-site counter.
            return false;
        }
''')
elif cell == 'C4':
    open(lib,'w').write(open(base_lib).read())
    patch(lib, '''        Err(error) => {
            inner.export_failed.fetch_add(1, Ordering::Relaxed);
            if !*disabled {''', '''        Err(error) => {
            // QFB-MUTANT-C4: report a failed write as a delivered record.
            inner.exported.fetch_add(1, Ordering::Relaxed);
            inner.export_failed.fetch_add(1, Ordering::Relaxed);
            if !*disabled {''')
else:
    raise SystemExit('bad cell')
print('mutant applied', cell)
