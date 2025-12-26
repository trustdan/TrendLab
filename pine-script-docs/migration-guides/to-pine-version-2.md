# To Pine Script® version 2

Source: https://www.tradingview.com/pine-script-docs/migration-guides/to-pine-version-2

---

[]()

[User Manual ](/pine-script-docs) / [Migration guides](/pine-script-docs/migration-guides/overview) / To Pine Script® version 2

[To Pine Script® version 2](#to-pine-script-version-2)
==========

Pine Script version 2 is fully backwards compatible with version 1. As a result, all v1 scripts can be converted to v2 by adding the `//@version=2` annotation to them.

An example v1 script:

[Pine Script®](https://tradingview.com/pine-script-docs)

Copied

`study("Simple Moving Average", shorttitle="SMA")  
src = close  
length = input(10)  
plot(sma(src, length))  
`

The converted v2 script:

[Pine Script®](https://tradingview.com/pine-script-docs)

Copied

`//@version=2  
study("Simple Moving Average", shorttitle="SMA")  
src = close  
length = input(10)  
plot(sma(src, length))  
`

[

Previous

####  To Pine Script® version 3  ####

](/pine-script-docs/migration-guides/to-pine-version-3)