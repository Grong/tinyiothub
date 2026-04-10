import type { TemplateResult } from "lit";
import { renderA2uiText } from "./text.js";
import { renderA2uiButton } from "./button.js";
import { renderA2uiCard } from "./card.js";
import { renderA2uiColumn } from "./column.js";
import { renderA2uiRow } from "./row.js";
import { renderA2uiDivider } from "./divider.js";
import { renderDeviceCard } from "./device-card.js";
import { renderDeviceTable } from "./device-table.js";
import { renderDataChart } from "./data-chart.js";
import { renderControlPanel } from "./control-panel.js";
import { renderProgressIndicator } from "./progress-indicator.js";
import { renderConfirmationDialog } from "./confirmation-dialog.js";

export type A2uiRenderer = (data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void) => TemplateResult;

export const a2uiCatalog: Record<string, A2uiRenderer> = {
  Text: renderA2uiText,
  Button: renderA2uiButton,
  Card: renderA2uiCard,
  Column: renderA2uiColumn,
  Row: renderA2uiRow,
  Divider: renderA2uiDivider,
  DeviceCard: renderDeviceCard,
  DeviceTable: renderDeviceTable,
  DataChart: renderDataChart,
  ControlPanel: renderControlPanel,
  ProgressIndicator: renderProgressIndicator,
  ConfirmationDialog: renderConfirmationDialog,
};
