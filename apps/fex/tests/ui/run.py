# Runs every UI scenario of Plume FEx and prints a summary.
#   cargo build --profile fast -p plume-fex --features test-hook
#   python apps/fex/tests/ui/run.py
import glob, os, subprocess, sys, time

here = os.path.dirname(os.path.abspath(__file__))
failed = []
total = 0
t0 = time.time()
for path in sorted(glob.glob(os.path.join(here, "test_*.py"))):
    name = os.path.basename(path)
    r = subprocess.run([sys.executable, path], capture_output=True, text=True, stdin=subprocess.DEVNULL)
    lines = r.stdout.splitlines()
    passed = sum(1 for line in lines if line.startswith("PASS "))
    bad = [line for line in lines if line.startswith("FAIL ")]
    total += passed + len(bad)
    ok = r.returncode == 0 and not bad and lines and lines[-1].startswith("ALL ")
    print("%-22s %s  (%d checks)" % (name, "ok" if ok else "FAILED", passed + len(bad)))
    if not ok:
        failed.append(name)
        for line in bad:
            print("    " + line)
        if r.stderr.strip():
            print("    " + r.stderr.strip().splitlines()[-1])
print("%d checks in %.0f s: %s" % (total, time.time() - t0, "all passed" if not failed else "FAILED: " + ", ".join(failed)))
sys.exit(1 if failed else 0)
