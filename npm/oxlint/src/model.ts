export interface PatinaPosition {
  line: number;
  column: number;
  offset: number;
}

export interface PatinaLocation {
  start: PatinaPosition;
  end: PatinaPosition;
}

export interface PatinaDiagnostic {
  rule: string;
  severity: "error" | "warning";
  message: string;
  location: PatinaLocation;
  help: string | null;
}

export interface PatinaLintResult {
  filename: string;
  errorCount: number;
  warningCount: number;
  diagnostics: PatinaDiagnostic[];
}

export interface PatinaRuleMeta {
  name: string;
  description: string;
  category: string;
  fixable: boolean;
  defaultSeverity: "error" | "warning";
  presets: PatinaPreset[];
}

export interface PatinaBinding {
  lintPatinaSfc(
    source: string,
    options?: {
      filename?: string;
      locale?: string;
      helpLevel?: HelpLevel;
      preset?: PatinaPreset;
      enabledRules?: string[];
      typeAware?: boolean;
      corsaPath?: string;
      componentNameInTemplateCasing?: ComponentNameInTemplateCasingOption;
      customEventNameCasing?: CustomEventNameCasingOption;
      htmlSelfClosing?: HtmlSelfClosingOption;
    },
  ): PatinaLintResult;
  getPatinaRules(): PatinaRuleMeta[];
}

export type HelpLevel = "none" | "short" | "full";
export type PatinaPreset =
  | "general-recommended"
  | "essential"
  | "ecosystem"
  | "incremental"
  | "opinionated"
  | "nuxt";

export interface PatinaSettings {
  locale?: string;
  helpLevel?: HelpLevel;
  preset?: PatinaPreset;
  typeAware?: boolean;
  corsaPath?: string;
}

export interface PatinaRuleOptions {
  componentNameInTemplateCasing?: ComponentNameInTemplateCasingOption;
  customEventNameCasing?: CustomEventNameCasingOption;
  htmlSelfClosing?: HtmlSelfClosingOption;
}

export interface LineColumn {
  line: number;
  column: number;
}

export type ComponentNameInTemplateCasingOption = "PascalCase" | "kebab-case";
export type CustomEventNameCasingOption = "camelCase" | "kebab-case";
export type HtmlSelfClosingStyle = "always" | "never" | "any";
export interface HtmlSelfClosingOption {
  html?: {
    void?: HtmlSelfClosingStyle;
    normal?: HtmlSelfClosingStyle;
    component?: HtmlSelfClosingStyle;
  };
  svg?: HtmlSelfClosingStyle;
  math?: HtmlSelfClosingStyle;
}

export type SfcBlockKind = "template" | "script" | "script-setup" | "style" | "custom";

export interface SfcBlock {
  kind: SfcBlockKind;
  name: string;
  content: string;
  contentStart: LineColumn;
  contentEnd: LineColumn;
}

export interface SingleScriptMap {
  block: SfcBlock;
  /**
   * SFC position of the extracted program's first character. Oxlint hands JS
   * plugins the block body without the newline that follows the open tag, so
   * this is not always `block.contentStart`.
   */
  scriptStart: LineColumn;
}
