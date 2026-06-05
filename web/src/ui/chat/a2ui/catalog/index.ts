import type { TemplateResult } from "lit";
import { renderA2uiText } from "./text.js";
import { renderA2uiButton } from "./button.js";
import { renderA2uiCard } from "./card.js";
import { renderA2uiColumn } from "./column.js";
import { renderA2uiRow } from "./row.js";
import { renderA2uiDivider } from "./divider.js";
import { renderA2uiImage } from "./image.js";
import { renderA2uiIcon } from "./icon.js";
import { renderA2uiList } from "./list.js";
import { renderA2uiTabs } from "./tabs.js";
import { renderA2uiModal } from "./modal.js";
import { renderA2uiTextField } from "./text-field.js";
import { renderA2uiCheckBox } from "./check-box.js";
import { renderA2uiChoicePicker } from "./choice-picker.js";
import { renderA2uiSlider } from "./slider.js";
import { renderA2uiDateTimeInput } from "./date-time-input.js";
import { renderDeviceCard } from "./device-card.js";
import { renderDeviceTable } from "./device-table.js";
import { renderDataChart } from "./data-chart.js";
import { renderControlPanel } from "./control-panel.js";
import { renderProgressIndicator } from "./progress-indicator.js";
import { renderConfirmationDialog } from "./confirmation-dialog.js";
import { renderAlarmCard } from "./alarm-card.js";
import { renderAlarmTable } from "./alarm-table.js";
import { renderStatCard } from "./stat-card.js";
import { renderStatRow } from "./stat-row.js";
import { renderScene3D } from "./scene-3d.js";

export type A2uiRenderer = (data: Record<string, unknown>, onAction?: (fn: string, args: Record<string, unknown>) => void) => TemplateResult;

export const a2uiCatalog: Record<string, A2uiRenderer> = {
  // Basic Catalog
  Text: renderA2uiText,
  Image: renderA2uiImage,
  Icon: renderA2uiIcon,
  Button: renderA2uiButton,
  Card: renderA2uiCard,
  List: renderA2uiList,
  Tabs: renderA2uiTabs,
  Modal: renderA2uiModal,
  Column: renderA2uiColumn,
  Row: renderA2uiRow,
  Divider: renderA2uiDivider,
  TextField: renderA2uiTextField,
  CheckBox: renderA2uiCheckBox,
  ChoicePicker: renderA2uiChoicePicker,
  Slider: renderA2uiSlider,
  DateTimeInput: renderA2uiDateTimeInput,
  // IoT Components
  DeviceCard: renderDeviceCard,
  DeviceTable: renderDeviceTable,
  DataChart: renderDataChart,
  ControlPanel: renderControlPanel,
  ProgressIndicator: renderProgressIndicator,
  ConfirmationDialog: renderConfirmationDialog,
  AlarmCard: renderAlarmCard,
  AlarmTable: renderAlarmTable,
  StatCard: renderStatCard,
  StatRow: renderStatRow,
  Scene3D: renderScene3D,
};
