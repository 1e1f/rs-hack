import { expect, test } from "bun:test";
import { render } from "@testing-library/react";
import { KindPill, Pill, StatusPill } from "./Pill";

test("Pill renders children and hue class", () => {
  const { getByText } = render(<Pill hue="open">Hello</Pill>);
  const el = getByText("Hello");
  expect(el.textContent).toBe("Hello");
  expect(el.className).toContain("text-st-open");
});

test("StatusPill maps known status to label", () => {
  const { getByText } = render(<StatusPill status="in-progress" />);
  expect(getByText("In flight").textContent).toBe("In flight");
});

test("KindPill returns null for missing kind", () => {
  const { container } = render(<KindPill kind={undefined} />);
  expect(container.firstChild).toBeNull();
});
