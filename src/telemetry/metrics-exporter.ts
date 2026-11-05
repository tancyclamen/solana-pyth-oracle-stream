export interface MetricSample {
  name: string;
  value: number;
  labels: Record<string, string>;
  timestamp: number;
}

export class PrometheusMetricsExporter {
  private samples: MetricSample[] = [];

  public recordHistogram(name: string, value: number, labels: Record<string, string> = {}): void {
    this.samples.push({
      name,
      value,
      labels,
      timestamp: Date.now()
    });
    if (this.samples.length > 500) {
      this.samples = this.samples.slice(-250);
    }
  }

  public exportMetrics(): string {
    return this.samples
      .map(s => {
        const labelStr = Object.entries(s.labels)
          .map(([k, v]) => `${k}="${v}"`)
          .join(',');
        return `${s.name}{${labelStr}} ${s.value} ${s.timestamp}`;
      })
      .join('\n');
  }
}
