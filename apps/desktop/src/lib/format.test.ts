import { describe, expect, it } from "vitest";
import {
  domainOf,
  formatBytes,
  formatDuration,
  formatSpeed,
  percent,
  relativeTime,
  statusLabel,
  statusTone,
  truncateMiddle,
} from "./format";

describe("format helpers", () => {
  it("formats bytes with binary units", () => {
    expect(formatBytes(null)).toBe("—");
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(1.5 * 1024 ** 3)).toBe("1.5 GB");
  });

  it("formats speeds and durations", () => {
    expect(formatSpeed(null)).toBe("—");
    expect(formatSpeed(0)).toBe("—");
    expect(formatSpeed(2048)).toBe("2.0 KB/s");
    expect(formatDuration(undefined)).toBe("—");
    expect(formatDuration(42)).toBe("42s");
    expect(formatDuration(125)).toBe("2m 5s");
    expect(formatDuration(3725)).toBe("1h 2m");
  });

  it("computes bounded percentages", () => {
    expect(percent({ downloaded_bytes: 50, total_bytes: 100 })).toBe(50);
    expect(percent({ downloaded_bytes: 150, total_bytes: 100 })).toBe(100);
    expect(percent({ downloaded_bytes: 5, total_bytes: null })).toBe(0);
  });

  it("labels states and tones", () => {
    expect(statusLabel("waiting_for_network")).toBe("Waiting for network");
    expect(statusLabel("custom_state")).toBe("custom state");
    expect(statusTone("failed")).toContain("danger");
    expect(statusTone("downloading")).toContain("accent");
  });

  it("extracts domains and truncates long URLs", () => {
    expect(domainOf("https://cdn.example.com/file.zip")).toBe("cdn.example.com");
    expect(domainOf("not a url")).toBe("not a url");
    expect(truncateMiddle("short", 20)).toBe("short");
    expect(truncateMiddle("https://example.com/a/very/long/path/to/file.zip", 20)).toContain("…");
  });

  it("renders relative times", () => {
    expect(relativeTime(new Date().toISOString())).toBe("just now");
    expect(relativeTime(new Date(Date.now() - 5 * 60_000).toISOString())).toBe("5m ago");
    expect(relativeTime(null)).toBe("—");
  });
});
