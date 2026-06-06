export type Stage = "queued" | "decoding" | "encoding" | "optimizing" | "done" | "error";

export interface FileItem {
  id: string;
  path: string;
  name: string;
  ext: string;
  stage: Stage;
  percent: number;
  originalBytes?: number;
  outputBytes?: number;
  outputPath?: string;
  error?: string;
}

export interface ConvertProgressPayload {
  id: string;
  stage: Stage;
  percent: number;
}

export interface ConvertResultPayload {
  id: string;
  output_path: string;
  original_bytes: number;
  output_bytes: number;
}
