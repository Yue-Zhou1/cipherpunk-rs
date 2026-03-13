declare module "react-cytoscapejs" {
  import type { ComponentType, CSSProperties } from "react";
  import type cytoscape, { ElementDefinition, LayoutOptions, Stylesheet } from "cytoscape";

  export type CytoscapeElements = ElementDefinition[] | cytoscape.ElementsDefinition;

  export interface CytoscapeComponentProps {
    elements?: CytoscapeElements;
    layout?: LayoutOptions;
    style?: CSSProperties;
    stylesheet?: Stylesheet[];
    className?: string;
    cy?: (core: cytoscape.Core) => void;
    minZoom?: number;
    maxZoom?: number;
    zoomingEnabled?: boolean;
    panningEnabled?: boolean;
    userZoomingEnabled?: boolean;
    userPanningEnabled?: boolean;
    boxSelectionEnabled?: boolean;
    autoungrabify?: boolean;
    autounselectify?: boolean;
  }

  const CytoscapeComponent: ComponentType<CytoscapeComponentProps>;

  export default CytoscapeComponent;
}
