import glob, os, statistics, sys

ROOT = sys.argv[1] if len(sys.argv) > 1 else '/tmp/vfe35'

def pct(xs, p):
    xs = sorted(xs)
    if not xs: return float('nan')
    k = (len(xs)-1) * p/100.0
    lo, hi = int(k), min(int(k)+1, len(xs)-1)
    return xs[lo] + (xs[hi]-xs[lo])*(k-lo)

for path in sorted(glob.glob(os.path.join(ROOT, '*.log'))):
    label = os.path.basename(path)[:-4]
    rows = []
    uploads = []
    meta = None
    for line in open(path):
        if 'BENCH,' in line:
            payload = line.split('BENCH,',1)[1].strip()
            parts = payload.split(',')
            if len(parts) == 14:
                rows.append([parts[0], int(parts[1])] + [float(x) for x in parts[2:]])
        if 'BENCHMETA,' in line:
            meta = line.split('BENCHMETA,',1)[1].strip().split(',')
        if 'BENCHUPLOAD,' in line:
            parts = line.split('BENCHUPLOAD,',1)[1].strip().split(',')
            uploads.append([float(parts[1])] + [int(x) for x in parts[2:]])
    if not rows:
        print(label, 'NO ROWS'); continue
    # rows: label, frame, interval, sim, advect, dot, trail, viewport, combined,
    #       trailsort, particles, dots, trails, arrows
    # Discard the first 2 s of wall time (startup / shader compile).
    cum = 0.0
    steady = []
    for r in rows:
        cum += r[2] / 1e6
        if cum >= 2.0:
            steady.append(r)
    if not steady:
        steady = rows
    iv = [r[2] for r in steady]
    phases = {name: [r[idx] for r in steady] for name, idx in
              [('sim',3),('advect',4),('dot',5),('trail',6),('viewport',7),
               ('combined',8),('trailsort',9)]}
    window = (steady[-1][1]-steady[0][1]) * statistics.median(iv) / 1e6
    up = [u[0] for u in uploads[90:]] or [u[0] for u in uploads]
    first = steady[0]
    print(f"{label:40s} n={len(steady):5d} win={window:5.1f}s "
          f"iv p50={pct(iv,50)/1000:6.2f} p95={pct(iv,95)/1000:6.2f} p99={pct(iv,99)/1000:6.2f} ms "
          f"-> {1e6/pct(iv,50):5.1f} fps")
    print(" " * 4 + "phases p50/p95 ms: " + " ".join(
        f"{k}={pct(v,50)/1000:.2f}/{pct(v,95)/1000:.2f}" for k, v in phases.items())
        + f"  upload={pct(up,50)/1000:.2f}  counts=P{int(first[10])} D{int(first[11])} T{int(first[12])} A{int(first[13])}")
