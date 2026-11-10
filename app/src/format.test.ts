import { describe, expect, it } from "vitest";
import { bars, budget, duration, watts } from "./format";

describe("format", () => {
  it("formats runtime", () => {
    expect(duration(4.866)).toBe("4h 52m");
    expect(duration(0.5)).toBe("30m");
    expect(duration(null)).toBeNull();
    expect(duration(0)).toBeNull();
    expect(duration(Infinity)).toBeNull();
    expect(duration(500)).toBe(">99h");
  });
  it("formats watts", () => {
    expect(watts(9.83)).toBe("9.8 W");
    expect(watts(null)).toBe("—");
  });
  it("maps signal and budget", () => {
    expect(bars(76)).toBe(4);
    expect(bars(10)).toBe(1);
    expect(budget(6)?.name).toBe("Deep Eco range");
    expect(budget(16)?.tone).toBe("red");
  });
});
