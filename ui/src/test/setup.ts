import "@testing-library/jest-dom/vitest";

if (!globalThis.ResizeObserver) {
  class ResizeObserverMock implements ResizeObserver {
    observe(_target: Element, _options?: ResizeObserverOptions) {}
    unobserve(_target: Element) {}
    disconnect() {}
  }

  // jsdom environment shim for libraries (ReactFlow) that require ResizeObserver.
  globalThis.ResizeObserver = ResizeObserverMock;
}
