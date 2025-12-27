import type { DateRange } from '../../../types';

interface DateRangeEditorProps {
  dateRange: DateRange;
  isActive: boolean;
  onChange: (dateRange: DateRange) => void;
}

export function DateRangeEditor({
  dateRange,
  isActive,
  onChange,
}: DateRangeEditorProps) {
  const handleStartChange = (value: string) => {
    onChange({ ...dateRange, start: value });
  };

  const handleEndChange = (value: string) => {
    onChange({ ...dateRange, end: value });
  };

  // Calculate date range span
  const startDate = new Date(dateRange.start);
  const endDate = new Date(dateRange.end);
  const days = Math.ceil((endDate.getTime() - startDate.getTime()) / (1000 * 60 * 60 * 24));
  const years = (days / 365).toFixed(1);

  // Quick presets
  const presets = [
    { label: '5Y', years: 5 },
    { label: '10Y', years: 10 },
    { label: '15Y', years: 15 },
    { label: '20Y', years: 20 },
  ];

  const applyPreset = (presetYears: number) => {
    const end = new Date();
    const start = new Date();
    start.setFullYear(start.getFullYear() - presetYears);
    onChange({
      start: start.toISOString().split('T')[0],
      end: end.toISOString().split('T')[0],
    });
  };

  return (
    <div className={`sweep-section ${isActive ? 'active' : ''}`}>
      <h3 className="sweep-section-title">Date Range</h3>

      <div className="date-inputs">
        <div className="date-row">
          <label htmlFor="start-date">Start</label>
          <input
            id="start-date"
            type="date"
            value={dateRange.start}
            onChange={(e) => handleStartChange(e.target.value)}
            className="date-input"
          />
        </div>

        <div className="date-row">
          <label htmlFor="end-date">End</label>
          <input
            id="end-date"
            type="date"
            value={dateRange.end}
            onChange={(e) => handleEndChange(e.target.value)}
            className="date-input"
          />
        </div>
      </div>

      <div className="date-summary">
        <span className="date-span">
          {years} years ({days.toLocaleString()} days)
        </span>
      </div>

      <div className="date-presets">
        {presets.map((preset) => (
          <button
            key={preset.label}
            className="preset-btn"
            onClick={() => applyPreset(preset.years)}
          >
            {preset.label}
          </button>
        ))}
      </div>

      <style>{`
        .date-inputs {
          display: flex;
          gap: var(--space-md);
        }
        .date-row {
          flex: 1;
          display: flex;
          flex-direction: column;
          gap: var(--space-xs);
        }
        .date-row label {
          font-size: var(--font-size-sm);
          color: var(--muted);
        }
        .date-input {
          padding: var(--space-xs) var(--space-sm);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          color: var(--fg);
          font-size: var(--font-size-sm);
        }
        .date-input:focus {
          outline: none;
          border-color: var(--blue);
        }
        .date-summary {
          margin-top: var(--space-sm);
          text-align: center;
        }
        .date-span {
          color: var(--cyan);
          font-weight: 500;
        }
        .date-presets {
          display: flex;
          gap: var(--space-xs);
          margin-top: var(--space-sm);
          justify-content: center;
        }
        .preset-btn {
          padding: var(--space-xs) var(--space-sm);
          background: var(--bg);
          border: 1px solid var(--border);
          border-radius: var(--radius-sm);
          color: var(--muted);
          font-size: var(--font-size-xs);
          cursor: pointer;
          transition: all 0.15s ease;
        }
        .preset-btn:hover {
          border-color: var(--blue);
          color: var(--fg);
        }
      `}</style>
    </div>
  );
}
