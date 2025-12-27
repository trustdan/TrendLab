import { VscTriangleUp, VscTriangleDown } from 'react-icons/vsc';
import type { SortMetric } from '../../../types';

interface MetricHeaderProps {
  metric: SortMetric;
  label: string;
  isActive: boolean;
  ascending: boolean;
  onClick: () => void;
}

export function MetricHeader({
  metric,
  label,
  isActive,
  ascending,
  onClick,
}: MetricHeaderProps) {
  // Determine aria-sort value
  const ariaSort = isActive
    ? ascending
      ? 'ascending'
      : 'descending'
    : undefined;

  return (
    <th
      className={`metric-header ${isActive ? 'active' : ''}`}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          onClick();
        }
      }}
      tabIndex={0}
      role="columnheader"
      aria-sort={ariaSort}
      title={`Sort by ${label}`}
    >
      <span className="label">{label}</span>
      <span className="sort-indicator" aria-hidden="true">
        {isActive && (ascending ? <VscTriangleUp /> : <VscTriangleDown />)}
      </span>

      <style>{`
        .metric-header {
          cursor: pointer;
          user-select: none;
          text-align: right !important;
        }

        .metric-header:hover {
          background: var(--bg-hover);
        }

        .metric-header.active {
          color: var(--cyan);
        }

        .metric-header .label {
          display: inline;
        }

        .metric-header .sort-indicator {
          display: inline-block;
          width: 16px;
          margin-left: 4px;
          vertical-align: middle;
        }

        .metric-header .sort-indicator svg {
          vertical-align: middle;
        }
      `}</style>
    </th>
  );
}
