import { useState, useCallback } from 'react';
import { VscStarFull, VscSync } from 'react-icons/vsc';
import { useAppStore } from '../../../store';
import type { AggregatedConfigResult, LeaderboardEntry } from '../../../types/yolo';

interface LeaderboardProps {
  isActive: boolean;
}

type LeaderboardTab = 'cross_symbol' | 'per_symbol';

export function Leaderboard({ isActive }: LeaderboardProps) {
  const {
    crossSymbolLeaderboard,
    leaderboard,
    yoloIteration,
    yoloTotalConfigsTested,
    setActivePanel,
    setChartMode,
  } = useAppStore();

  const [activeTab, setActiveTab] = useState<LeaderboardTab>('cross_symbol');

  const handleSelectCrossSymbol = useCallback(
    (entry: AggregatedConfigResult) => {
      // TODO: Set selected config for chart comparison
      console.log('Selected cross-symbol entry:', entry);
      setChartMode('equity');
      setActivePanel('chart');
    },
    [setChartMode, setActivePanel]
  );

  const handleSelectPerSymbol = useCallback(
    (entry: LeaderboardEntry) => {
      // TODO: Set selected result for chart view
      console.log('Selected per-symbol entry:', entry);
      setChartMode('equity');
      setActivePanel('chart');
    },
    [setChartMode, setActivePanel]
  );

  const crossEntries = crossSymbolLeaderboard?.entries ?? [];
  const perSymbolEntries = leaderboard?.entries ?? [];
  const hasData = crossEntries.length > 0 || perSymbolEntries.length > 0;

  const getRankClass = (rank: number) => {
    if (rank === 1) return 'gold';
    if (rank === 2) return 'silver';
    if (rank === 3) return 'bronze';
    return '';
  };

  const formatPct = (val: number) => `${(val * 100).toFixed(2)}%`;
  const formatRatio = (val: number) => val.toFixed(2);

  return (
    <div className={`leaderboard ${isActive ? 'active' : ''}`}>
      <div className="leaderboard-header">
        <div className="header-left">
          <VscStarFull className="header-icon" />
          <span className="header-title">YOLO Leaderboard</span>
        </div>
        <div className="header-stats">
          <span className="stat">
            <VscSync className="stat-icon" />
            {yoloIteration} iterations
          </span>
          <span className="stat">{yoloTotalConfigsTested.toLocaleString()} tested</span>
        </div>
      </div>

      <div className="leaderboard-tabs">
        <button
          className={`tab ${activeTab === 'cross_symbol' ? 'active' : ''}`}
          onClick={() => setActiveTab('cross_symbol')}
        >
          Cross-Symbol ({crossEntries.length})
        </button>
        <button
          className={`tab ${activeTab === 'per_symbol' ? 'active' : ''}`}
          onClick={() => setActiveTab('per_symbol')}
        >
          Per-Symbol ({perSymbolEntries.length})
        </button>
      </div>

      {!hasData && (
        <div className="leaderboard-empty">
          <VscStarFull size={48} />
          <p>No leaderboard data yet</p>
          <p className="hint">Run YOLO mode to discover winning configurations</p>
        </div>
      )}

      {hasData && activeTab === 'cross_symbol' && (
        <div className="leaderboard-table-container">
          <table className="leaderboard-table">
            <thead>
              <tr>
                <th>#</th>
                <th>Strategy</th>
                <th>Config</th>
                <th>Symbols</th>
                <th>Avg Sharpe</th>
                <th>Min Sharpe</th>
                <th>CAGR</th>
                <th>Hit Rate</th>
              </tr>
            </thead>
            <tbody>
              {crossEntries.map((entry) => (
                <tr
                  key={`${entry.strategyType}-${entry.configId}`}
                  className={`row ${getRankClass(entry.rank)}`}
                  onClick={() => handleSelectCrossSymbol(entry)}
                >
                  <td className="rank-cell">
                    <span className={`rank-badge ${getRankClass(entry.rank)}`}>
                      {entry.rank}
                    </span>
                  </td>
                  <td className="strategy-cell">{entry.strategyType}</td>
                  <td className="config-cell" title={entry.configId}>
                    {entry.configId.slice(0, 12)}...
                  </td>
                  <td className="symbols-cell">{entry.symbols.length}</td>
                  <td className="metric-cell positive">
                    {formatRatio(entry.aggregateMetrics.avgSharpe)}
                  </td>
                  <td className="metric-cell">
                    {formatRatio(entry.aggregateMetrics.minSharpe)}
                  </td>
                  <td className="metric-cell positive">
                    {formatPct(entry.aggregateMetrics.geoMeanCagr)}
                  </td>
                  <td className="metric-cell">
                    {formatPct(entry.aggregateMetrics.hitRate)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {hasData && activeTab === 'per_symbol' && (
        <div className="leaderboard-table-container">
          <table className="leaderboard-table">
            <thead>
              <tr>
                <th>#</th>
                <th>Symbol</th>
                <th>Strategy</th>
                <th>Config</th>
                <th>Sharpe</th>
                <th>CAGR</th>
                <th>Max DD</th>
                <th>Iteration</th>
              </tr>
            </thead>
            <tbody>
              {perSymbolEntries.map((entry) => (
                <tr
                  key={`${entry.symbol}-${entry.strategyType}-${entry.configId}`}
                  className={`row ${getRankClass(entry.rank)}`}
                  onClick={() => handleSelectPerSymbol(entry)}
                >
                  <td className="rank-cell">
                    <span className={`rank-badge ${getRankClass(entry.rank)}`}>
                      {entry.rank}
                    </span>
                  </td>
                  <td className="symbol-cell">{entry.symbol ?? '-'}</td>
                  <td className="strategy-cell">{entry.strategyType}</td>
                  <td className="config-cell" title={entry.configId}>
                    {entry.configId.slice(0, 12)}...
                  </td>
                  <td className="metric-cell positive">
                    {formatRatio(entry.sharpe)}
                  </td>
                  <td className="metric-cell positive">
                    {formatPct(entry.cagr)}
                  </td>
                  <td className="metric-cell negative">
                    {formatPct(entry.maxDrawdown)}
                  </td>
                  <td className="iteration-cell">{entry.iteration}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <style>{`
        .leaderboard {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: var(--bg-secondary);
          border-radius: var(--radius-md);
          border: 1px solid var(--border);
          overflow: hidden;
        }

        .leaderboard.active {
          border-color: var(--yellow);
        }

        .leaderboard-header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          padding: var(--space-sm) var(--space-md);
          background: var(--surface);
          border-bottom: 1px solid var(--border);
        }

        .header-left {
          display: flex;
          align-items: center;
          gap: var(--space-sm);
        }

        .header-icon {
          color: var(--yellow);
          font-size: 18px;
        }

        .header-title {
          font-weight: 600;
          color: var(--fg);
        }

        .header-stats {
          display: flex;
          gap: var(--space-md);
          font-size: var(--font-size-sm);
          color: var(--muted);
        }

        .stat {
          display: flex;
          align-items: center;
          gap: var(--space-xs);
        }

        .stat-icon {
          font-size: 12px;
        }

        .leaderboard-tabs {
          display: flex;
          padding: var(--space-xs);
          gap: var(--space-xs);
          background: var(--bg);
          border-bottom: 1px solid var(--border);
        }

        .tab {
          flex: 1;
          padding: var(--space-xs) var(--space-sm);
          font-size: var(--font-size-sm);
          background: transparent;
          border: 1px solid transparent;
          border-radius: var(--radius-sm);
          color: var(--muted);
          cursor: pointer;
          transition: all 0.15s;
        }

        .tab:hover {
          color: var(--fg);
          background: var(--bg-hover);
        }

        .tab.active {
          background: var(--yellow);
          color: var(--bg);
          font-weight: 600;
        }

        .leaderboard-empty {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          gap: var(--space-md);
          padding: var(--space-xl);
          color: var(--muted);
          text-align: center;
        }

        .leaderboard-empty .hint {
          font-size: var(--font-size-sm);
          opacity: 0.7;
        }

        .leaderboard-table-container {
          flex: 1;
          overflow: auto;
        }

        .leaderboard-table {
          width: 100%;
          border-collapse: collapse;
          font-size: var(--font-size-sm);
        }

        .leaderboard-table th {
          position: sticky;
          top: 0;
          padding: var(--space-xs) var(--space-sm);
          background: var(--surface);
          text-align: left;
          font-weight: 600;
          color: var(--muted);
          border-bottom: 1px solid var(--border);
          white-space: nowrap;
        }

        .leaderboard-table td {
          padding: var(--space-xs) var(--space-sm);
          border-bottom: 1px solid var(--border);
          white-space: nowrap;
        }

        .leaderboard-table .row {
          cursor: pointer;
          transition: background 0.15s;
        }

        .leaderboard-table .row:hover {
          background: var(--bg-hover);
        }

        .leaderboard-table .row.gold {
          background: rgba(255, 215, 0, 0.1);
        }

        .leaderboard-table .row.silver {
          background: rgba(192, 192, 192, 0.1);
        }

        .leaderboard-table .row.bronze {
          background: rgba(205, 127, 50, 0.1);
        }

        .rank-cell {
          width: 40px;
          text-align: center;
        }

        .rank-badge {
          display: inline-flex;
          align-items: center;
          justify-content: center;
          width: 24px;
          height: 24px;
          border-radius: 50%;
          font-weight: 700;
          font-size: var(--font-size-xs);
          background: var(--muted);
          color: var(--bg);
        }

        .rank-badge.gold {
          background: #ffd700;
          color: #333;
        }

        .rank-badge.silver {
          background: #c0c0c0;
          color: #333;
        }

        .rank-badge.bronze {
          background: #cd7f32;
          color: #fff;
        }

        .strategy-cell {
          font-weight: 500;
          color: var(--cyan);
        }

        .symbol-cell {
          font-weight: 500;
          color: var(--blue);
        }

        .config-cell {
          font-family: var(--font-mono);
          font-size: var(--font-size-xs);
          color: var(--muted);
        }

        .symbols-cell {
          text-align: center;
        }

        .metric-cell {
          font-family: var(--font-mono);
          text-align: right;
        }

        .metric-cell.positive {
          color: var(--green);
        }

        .metric-cell.negative {
          color: var(--red);
        }

        .iteration-cell {
          text-align: center;
          color: var(--muted);
        }
      `}</style>
    </div>
  );
}
