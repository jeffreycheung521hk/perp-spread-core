#!/usr/bin/env python3
"""Independent Decimal oracle for the Rust TSV bridge; stdlib only.

Synthetic deterministic cases verify arithmetic and filtering, NOT market edge.
The reference keeps full decimal precision instead of copying fixed-point code.
"""
import argparse
from decimal import Decimal as D, ROUND_DOWN, getcontext
from pathlib import Path
import random
import subprocess

getcontext().prec = 60
BPS = D(10000)


def prices(text):
    return {D(p): D(q) for p, q in (level.split("@") for level in text.split(","))}


def walk(levels, quantity, buy):
    left, cash = quantity, D(0)
    for price in sorted(levels, reverse=not buy)[:20]:
        amount = min(left, levels[price])
        cash += amount * price
        left -= amount
        if left == 0:
            return cash
    raise ValueError("insufficient depth")


def reference(fields):
    lb, la, sb, sa = map(prices, fields[13:17])
    mid = (max(lb) + min(la) + max(sb) + min(sa)) / 4
    step = D(fields[4])
    quantity = ((D(fields[3]) / mid / step).to_integral_value(rounding=ROUND_DOWN) * step
                if fields[2] == "auto" else D(fields[2]))
    if quantity <= 0 or quantity % step:
        raise ValueError("bad size")
    a = walk(la, quantity, True)
    b = walk(sb, quantity, False)
    c = walk(lb, quantity, False)
    d = walk(sa, quantity, True)
    ref = quantity * mid
    fee = ((a + c) * D(fields[5]) + (b + d) * D(fields[6])) / BPS
    entry, exit_debit = (b-a)/ref*BPS, (d-c)/ref*BPS
    slip = max((a/(quantity*min(la))-1)*BPS, (1-b/(quantity*max(sb)))*BPS,
               (1-c/(quantity*max(lb)))*BPS, (d/(quantity*min(sa))-1)*BPS)
    long_spread = (min(la)-max(lb))/((min(la)+max(lb))/2)*BPS
    short_spread = (min(sa)-max(sb))/((min(sa)+max(sb))/2)*BPS
    net = entry - D(fields[7]) - fee/ref*BPS - D(fields[8])
    now, long_time, short_time = map(int, fields[10:13])
    if max(long_time, short_time) > now:
        reason = "future_timestamp"
    elif max(now-long_time, now-short_time) > 1500:
        reason = "stale_book"
    elif abs(long_time-short_time) > 750:
        reason = "pair_skew"
    elif slip > 10:
        reason = "walk_slippage"
    elif max(long_spread, short_spread) > 20:
        reason = "venue_spread"
    elif net < D(fields[9]):
        reason = "insufficient_edge"
    else:
        reason = "eligible"
    return {"quantity": quantity, "entry": entry, "exit": exit_debit, "fee_bps": fee/ref*BPS,
            "slippage": slip, "vwaps": [a/quantity,b/quantity,c/quantity,d/quantity],
            "net_bps": net, "reason": reason, "gross": c-a+b-d, "fees": fee,
            "net_cash": c-a+b-d-fee}


def fixtures():
    randomizer = random.Random(521)
    for i in range(256):
        center = D(randomizer.randint(9000, 25000)) / 100
        other = center + D(randomizer.randint(-80, 80)) / 100

        def side(mid, buy):
            return ",".join(str(mid + (1 if buy else -1)*D(j+1)/100) + "@"
                            + str(D(randomizer.randint(1, 40))/10) for j in range(5))

        now, long_time, short_time = 1000, 1000, 1000
        if i % 29 == 0:
            now = 3000
        elif i % 31 == 0:
            short_time = 1900
            now = 1900
        elif i % 37 == 0:
            now = 999
        yield [str(i), "hyperliquid" if i % 2 else "dydx",
               "auto" if i % 3 == 0 else ["1.123","3","7"][i % 3],
               "1000.000000000001", "0.001", "4.5" if i % 2 else "2.25",
               "5" if i % 2 else "1.6", str(D(randomizer.randint(-200,200))/10),
               "2", "5", str(now), str(long_time), str(short_time),
               side(center,False), side(center,True), side(other,False), side(other,True)]


def close(actual, expected, tolerance, label):
    if abs(D(actual)-expected) > D(tolerance):
        raise AssertionError(f"{label}: Rust={actual}, Decimal={expected}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path,
                        default=Path(__file__).resolve().parents[1]/"target"/"debug"/"spread-core")
    args = parser.parse_args()
    cases = list(fixtures())
    # A malformed request followed by a valid request must not kill the process.
    cases.append(["malformed", "request"])
    cases.append(["recovery", *cases[1][1:]])
    process = subprocess.run([str(args.binary)], input="".join("\t".join(f)+"\n" for f in cases),
                             text=True, capture_output=True, check=True)
    rows = [line.split("\t") for line in process.stdout.splitlines()]
    assert len(rows) == len(cases), (len(rows), len(cases), process.stderr)
    successes, errors = 0, 0
    for f, row in zip(cases, rows):
        assert row[0] == f[0], (f[0], row)
        try:
            if len(f) != 17:
                raise ValueError("malformed request")
            expected = reference(f)
        except ValueError:
            assert row[1] == "ERR", (f[0], row)
            errors += 1
            continue
        assert row[1] == "OK" and len(row) == 17, (f[0], row)
        close(row[2], expected["quantity"], "0", "quantity")
        for offset, key in [(3,"entry"),(4,"exit"),(5,"fee_bps"),(6,"slippage")]:
            close(row[offset],expected[key],"0.000002",key)
        # Entry is rounded down, costs up: the result cannot overstate edge.
        assert D(row[3]) <= expected["entry"] + D("1e-45")
        assert D(row[4]) >= expected["exit"] - D("1e-45")
        assert D(row[5]) >= expected["fee_bps"] - D("1e-45")
        assert D(row[6]) >= expected["slippage"] - D("1e-45")
        for actual, exact in zip(row[7:11],expected["vwaps"]):
            close(actual,exact,"0.000000000001","VWAP display")
        close(row[11],expected["net_bps"],"0.000003","net bps")
        assert D(row[11]) <= expected["net_bps"] + D("1e-45")
        assert row[12] == str(expected["reason"] == "eligible").lower(), (f[0],row,expected)
        assert row[13] == expected["reason"], (f[0],row,expected)
        for offset,key in [(14,"gross"),(15,"fees"),(16,"net_cash")]:
            close(row[offset],expected[key],"0.000000000004",key)
        assert D(row[14])-D(row[15]) == D(row[16])
        successes += 1
    print(f"PASS: {len(cases)} synthetic cases; {successes} quote comparisons, {errors} expected errors.")
    print("VWAP, four fees, conservative bps rounding, filters, and cash reconciliation verified.")


if __name__ == "__main__":
    main()
