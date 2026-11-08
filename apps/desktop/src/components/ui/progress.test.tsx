import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ProgressBar } from "./progress";

describe("ProgressBar", () => {
  it("renders a solid bar for a simple percentage", () => {
    const { container } = render(<ProgressBar value={42} />);
    const bars = container.querySelectorAll("div");
    expect(bars.length).toBeGreaterThan(0);
    expect(container.innerHTML).toContain("width: 42%");
  });

  it("renders one cell per segment when segment data exists", () => {
    const { container } = render(
      <ProgressBar
        value={50}
        segments={[
          { index: 0, start: 0, end: 99, downloaded: 100 },
          { index: 1, start: 100, end: 199, downloaded: 0 },
        ]}
      />,
    );
    const cell = container.firstElementChild as HTMLElement;
    expect(cell.children).toHaveLength(2);
  });

  it("renders an indeterminate animation", () => {
    const { container } = render(<ProgressBar value={0} indeterminate />);
    expect(container.querySelector(".animate-indeterminate")).not.toBeNull();
  });
});
