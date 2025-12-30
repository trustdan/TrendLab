#!/usr/bin/env python3
"""
Send email notification for TrendLab daily signals using Resend.

Usage:
    python send_signal_email.py reports/scans/2025-12-29.json

Environment variables:
    RESEND_API_KEY   - Your Resend API key
    ALERT_EMAIL_TO   - Recipient email address
    ALERT_EMAIL_FROM - Sender email address (must be from verified domain in Resend)
"""

import json
import os
import sys
from datetime import datetime
from typing import Any

import requests


def load_scan_results(path: str) -> dict[str, Any]:
    """Load scan results from JSON file."""
    with open(path, "r") as f:
        return json.load(f)


def format_html_email(data: dict[str, Any]) -> str:
    """Format scan results as HTML email."""
    scan_date = data.get("scan_date", "Unknown")
    watchlist_name = data.get("watchlist_name", "Unknown")
    signals = data.get("signals", [])
    summary = data.get("summary", {})

    # Filter to actionable signals only
    entries = [s for s in signals if s.get("signal") == "entry"]
    exits = [s for s in signals if s.get("signal") == "exit"]

    if not entries and not exits:
        return ""  # No email needed

    html = f"""
<!DOCTYPE html>
<html>
<head>
    <style>
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; }}
        .header {{ background: #1a1a2e; color: #fff; padding: 20px; text-align: center; }}
        .header h1 {{ margin: 0; font-size: 24px; }}
        .header p {{ margin: 5px 0 0; opacity: 0.8; }}
        .content {{ padding: 20px; max-width: 600px; margin: 0 auto; }}
        .summary {{ background: #f5f5f5; padding: 15px; border-radius: 8px; margin-bottom: 20px; }}
        .summary-stats {{ display: flex; gap: 20px; }}
        .stat {{ text-align: center; }}
        .stat-value {{ font-size: 28px; font-weight: bold; }}
        .stat-label {{ font-size: 12px; color: #666; }}
        .entry {{ color: #22c55e; }}
        .exit {{ color: #ef4444; }}
        table {{ width: 100%; border-collapse: collapse; margin: 10px 0; }}
        th, td {{ padding: 10px; text-align: left; border-bottom: 1px solid #eee; }}
        th {{ background: #f9f9f9; font-weight: 600; }}
        .signal-entry {{ color: #22c55e; font-weight: bold; }}
        .signal-exit {{ color: #ef4444; font-weight: bold; }}
        .footer {{ text-align: center; padding: 20px; color: #666; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>TrendLab Daily Signals</h1>
        <p>{watchlist_name} - {scan_date}</p>
    </div>
    <div class="content">
        <div class="summary">
            <div class="summary-stats">
                <div class="stat">
                    <div class="stat-value entry">{len(entries)}</div>
                    <div class="stat-label">ENTRIES</div>
                </div>
                <div class="stat">
                    <div class="stat-value exit">{len(exits)}</div>
                    <div class="stat-label">EXITS</div>
                </div>
                <div class="stat">
                    <div class="stat-value">{summary.get('total_tickers', 0)}</div>
                    <div class="stat-label">TOTAL TICKERS</div>
                </div>
            </div>
        </div>
"""

    if entries:
        html += """
        <h2 style="color: #22c55e;">Entry Signals</h2>
        <table>
            <tr>
                <th>Symbol</th>
                <th>Strategy</th>
                <th>Close</th>
            </tr>
"""
        for sig in entries:
            html += f"""
            <tr>
                <td><strong>{sig['symbol']}</strong></td>
                <td>{sig['strategy']} ({sig['params']})</td>
                <td>${sig['close_price']:.2f}</td>
            </tr>
"""
        html += "        </table>\n"

    if exits:
        html += """
        <h2 style="color: #ef4444;">Exit Signals</h2>
        <table>
            <tr>
                <th>Symbol</th>
                <th>Strategy</th>
                <th>Close</th>
            </tr>
"""
        for sig in exits:
            html += f"""
            <tr>
                <td><strong>{sig['symbol']}</strong></td>
                <td>{sig['strategy']} ({sig['params']})</td>
                <td>${sig['close_price']:.2f}</td>
            </tr>
"""
        html += "        </table>\n"

    # Errors section
    errors = summary.get("errors", [])
    if errors:
        html += """
        <h3 style="color: #f59e0b;">Scan Errors</h3>
        <ul style="color: #666; font-size: 14px;">
"""
        for err in errors[:5]:  # Limit to first 5 errors
            if isinstance(err, dict):
                html += f"            <li>{err.get('symbol', '?')}: {err.get('error', 'Unknown error')}</li>\n"
            else:
                html += f"            <li>{err}</li>\n"
        if len(errors) > 5:
            html += f"            <li>... and {len(errors) - 5} more</li>\n"
        html += "        </ul>\n"

    html += f"""
    </div>
    <div class="footer">
        <p>Generated by TrendLab at {data.get('scan_timestamp', datetime.now().isoformat())}</p>
        <p>This is an automated signal notification. Not financial advice.</p>
    </div>
</body>
</html>
"""
    return html


def send_email(to_email: str, from_email: str, subject: str, html_content: str) -> bool:
    """Send email via Resend API."""
    api_key = os.environ.get("RESEND_API_KEY")
    if not api_key:
        print("Error: RESEND_API_KEY environment variable not set")
        return False

    response = requests.post(
        "https://api.resend.com/emails",
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        json={
            "from": from_email,
            "to": [to_email],
            "subject": subject,
            "html": html_content,
        },
    )

    if response.status_code == 200:
        print(f"Email sent successfully. ID: {response.json().get('id')}")
        return True
    else:
        print(f"Error sending email: {response.status_code} - {response.text}")
        return False


def main():
    if len(sys.argv) < 2:
        print("Usage: python send_signal_email.py <scan_results.json>")
        sys.exit(1)

    scan_file = sys.argv[1]
    if not os.path.exists(scan_file):
        print(f"Error: File not found: {scan_file}")
        sys.exit(1)

    # Load environment variables
    to_email = os.environ.get("ALERT_EMAIL_TO")
    from_email = os.environ.get("ALERT_EMAIL_FROM")

    if not to_email or not from_email:
        print("Error: ALERT_EMAIL_TO and ALERT_EMAIL_FROM must be set")
        sys.exit(1)

    # Load and process scan results
    data = load_scan_results(scan_file)
    html_content = format_html_email(data)

    if not html_content:
        print("No actionable signals found. Skipping email.")
        sys.exit(0)

    # Build subject line
    summary = data.get("summary", {})
    entries = summary.get("entry_signals", 0)
    exits = summary.get("exit_signals", 0)
    scan_date = data.get("scan_date", "Unknown")

    subject = f"TrendLab Signals: {entries} entries, {exits} exits ({scan_date})"

    # Send email
    success = send_email(to_email, from_email, subject, html_content)
    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
