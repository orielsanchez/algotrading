# AlgoTrading Bot Development Partnership

We're building a production-quality momentum-based algorithmic trading bot together. Your role is to create maintainable, efficient, and SAFE trading solutions while catching potential issues early.

When you seem stuck or overly complex, I'll redirect you - my guidance helps you stay on track.

## CRITICAL TRADING SAFETY RULES

**NEVER compromise on trading safety:**
- ALWAYS use paper trading ports (7497/7496) during development
- NEVER hardcode live trading ports (7496/7495) without explicit confirmation
- ALWAYS validate order parameters before submission
- NEVER bypass risk limits or position sizing controls
- ALWAYS log all trading decisions and order submissions

## AUTOMATED CHECKS ARE MANDATORY

**ALL hook issues are BLOCKING - EVERYTHING must be GREEN!**  
No errors. No formatting issues. No linting problems. Zero tolerance.  
These are not suggestions. Fix ALL issues before continuing.

## CRITICAL WORKFLOW - ALWAYS FOLLOW THIS!

### Research → Plan → Implement

**NEVER JUMP STRAIGHT TO CODING!** Always follow this sequence:

1. **Research**: Explore the codebase, understand existing patterns
2. **Plan**: Create a detailed implementation plan and verify it with me
3. **Implement**: Execute the plan with validation checkpoints

When asked to implement any feature, you'll first say: "Let me research the codebase and create a plan before implementing."

For complex architectural decisions or challenging problems, use enhanced thinking tools to engage maximum reasoning capacity. Say: "Let me think deeply about this architecture before proposing a solution."

### USE TASK DELEGATION!

_Leverage Claude Code's capabilities strategically_ for better results:

- Break complex tasks into focused investigations
- Use systematic workflows for comprehensive analysis
- Delegate research tasks: "Let me investigate the database schema while analyzing the API structure"
- For complex refactors: Identify changes first, then implement systematically

Use the Task tool and systematic workflows whenever a problem has multiple independent parts.

### Enhanced Reality Checkpoints

**Stop and validate** at these moments:

- After implementing a complete feature
- Before starting a new major component
- When something feels wrong
- Before declaring "done"
- **WHEN HOOKS FAIL WITH ERRORS** (BLOCKING)

**Knowledge checkpoints:**
- After every major component: Explain the design choices made
- Before declaring "done": Can I implement this again without AI?
- Weekly: Review and explain recent patterns learned
- Monthly: Implement something similar from scratch to test retention

Run your project's quality checks (tests, linting, formatting)

> Why: You can lose track of what's actually working. These checkpoints prevent cascading failures and knowledge brownouts.

### CRITICAL: Hook Failures Are BLOCKING

**When hooks report ANY issues (exit code 2), you MUST:**

1. **STOP IMMEDIATELY** - Do not continue with other tasks
2. **FIX ALL ISSUES** - Address every issue until everything is GREEN
3. **VERIFY THE FIX** - Re-run the failed command to confirm it's fixed
4. **CONTINUE ORIGINAL TASK** - Return to what you were doing before the interrupt
5. **NEVER IGNORE** - There are NO warnings, only requirements

This includes:

- Formatting issues (prettier, black, rustfmt, etc.)
- Linting violations (eslint, flake8, clippy, etc.) 
- Forbidden patterns (defined by your project)
- ALL other quality checks

Your code must be 100% clean. No exceptions.

**Recovery Protocol:**

- When interrupted by a hook failure, maintain awareness of your original task
- After fixing all issues and verifying the fix, continue where you left off
- Use the todo list to track both the fix and your original task

## Knowledge Preservation Protocol

### Before AI Assistance:
- State your hypothesis about the problem/approach
- Identify which concepts you want to understand deeply
- Set learning objectives: "I want to understand X pattern"

### During Implementation:
- Explain the "why" behind each architectural decision
- Connect new patterns to existing knowledge
- Document mental models and intuition being built

### After Completion:
- Summarize key insights gained
- Update personal knowledge base with new patterns
- Identify areas for deeper independent study

## Test-Driven Development Protocol

**"Write the test, let AI satisfy the contract" - TDD with AI reduces debugging by 90%**

### The TDD-AI Feedback Loop:

1. **RED**: Write a failing test that defines the exact behavior
   - Be specific about inputs, outputs, and edge cases
   - Test the interface you wish existed
   - Document assumptions and constraints in tests

2. **GREEN**: Let AI implement the minimal code to pass
   - Provide the failing test as context
   - Ask AI to implement ONLY what's needed to pass
   - Resist over-engineering at this stage

3. **REFACTOR**: Improve design with test safety net
   - Clean up implementation with AI assistance
   - Tests ensure behavior preservation
   - Extract patterns and improve architecture

### TDD Commands Integration:
- Use `/tdd <feature>` to start test-first development
- All `/next` commands should begin with test design
- `/check` validates both implementation AND test quality

### TDD Learning Objectives:
- **Requirements Clarity**: Tests force precise thinking about behavior
- **Interface Design**: Write tests for the API you want to use
- **Regression Protection**: Changes can't break existing behavior
- **Documentation**: Tests serve as executable specifications

### Senior-Level TDD Thinking:
- Tests reveal design problems before implementation
- Good tests enable fearless refactoring
- Test structure mirrors system architecture
- Edge cases in tests prevent production surprises

**Why This Works With AI:**
- Tests provide unambiguous specifications
- AI can't misinterpret test requirements
- Failing tests guide AI toward correct solutions
- Passing tests validate AI implementations

## Working Memory Management

### When context gets long:

- Re-read this CLAUDE.md file
- Summarize progress in a PROGRESS.md file
- Document current state before major changes

### Maintain TODO.md:

```
## Current Task
- [ ] What we're doing RIGHT NOW

## Completed
- [x] What's actually done and tested

## Next Steps
- [ ] What comes next
```

## Language-Specific Quality Rules

### UNIVERSAL FORBIDDEN PATTERNS:

- **NO emojis** in code, comments, documentation, commit messages, or any project files
- **NO Claude attribution** in commit messages ("Generated with Claude Code", "Co-Authored-By: Claude", etc.)
- **NO** keeping old and new code together - delete when replacing
- **NO** migration functions or compatibility layers
- **NO** versioned function names (processV2, handleNew, etc.)
- **NO** TODOs in final production code
- **NO** println!/dbg! macros in production code
- **NO** hardcoded secrets or API keys
- **NO** broad exception catching without specific handling

### Rust-Specific Quality Standards:

**CRITICAL for Trading Safety:**
- **NO unwrap()** - use proper error handling with Result<T, E>
- **NO expect()** - handle errors explicitly, don't panic
- **NO panic!()** - trading bots must never panic in production
- **NO println!/dbg!** - use log crate (error!, warn!, info!, debug!)
- **ALWAYS** use `?` operator or match for error propagation
- **ALWAYS** use anyhow::Result for error handling consistency
- **ALWAYS** validate numeric conversions (no unchecked as casts)

**Code Quality:**
- Use `clippy` with `#![warn(clippy::all, clippy::pedantic)]`
- Run `cargo fmt` before any commit
- Use strong typing - avoid generic numeric types when specific
- Prefer exhaustive match statements over if-else chains
- Use const/static for configuration constants

**AUTOMATED ENFORCEMENT**: Quality hooks will BLOCK commits that violate these rules.  
When you see "FORBIDDEN PATTERN", you MUST fix it immediately!

### Universal Quality Standards:

- **Delete** old code when replacing it
- **Meaningful names**: `user_id` not `id`, `process_payment` not `do_stuff`
- **Early returns** to reduce nesting depth
- **Proper error handling** for your language (exceptions, Result types, etc.)
- **Comprehensive tests** for complex logic
- **Consistent code style** following project/language conventions
- **Clear separation of concerns** - single responsibility principle

### Example Patterns:

**JavaScript/TypeScript:**
```javascript
// GOOD: Proper error handling
async function fetchUserData(id: string): Promise<User | null> {
  try {
    const response = await fetch(`/api/users/${id}`);
    if (!response.ok) return null;
    return await response.json();
  } catch (error) {
    console.error('Failed to fetch user:', error);
    return null;
  }
}

// BAD: No error handling
async function fetchUserData(id: string): Promise<User> {
  const response = await fetch(`/api/users/${id}`);
  return await response.json(); // Can throw!
}
```

**Python:**
```python
# GOOD: Proper error handling
def parse_config(path: Path) -> Optional[Config]:
    try:
        with open(path) as f:
            return Config.from_json(f.read())
    except (FileNotFoundError, json.JSONDecodeError) as e:
        logger.error(f"Config parse failed: {e}")
        return None

# BAD: Bare except
def parse_config(path: Path) -> Config:
    try:
        with open(path) as f:
            return Config.from_json(f.read())
    except:  # Too broad!
        return Config()
```

## Implementation Standards

### Our code is complete when:

- All linters pass with zero issues
- All tests pass
- Feature works end-to-end
- Old code is deleted
- Documentation on all public items

### Testing Strategy for Trading Systems

**CRITICAL: All trading logic MUST be thoroughly tested!**

- **Trading algorithms** → Write tests first (TDD) with market scenarios
- **Order management** → Mock TWS API responses, test all edge cases
- **Risk controls** → Test limit breaches, position sizing, exposure
- **Market data** → Test data validation, missing data handling
- **Portfolio calculations** → Test P&L, position tracking, rebalancing

**Test Categories:**
- **Unit Tests**: Individual functions (momentum calculations, order validation)
- **Integration Tests**: Module interactions (strategy → orders → portfolio)
- **Scenario Tests**: Market conditions (gaps, halts, disconnections)
- **Backtesting**: Historical data validation before live deployment

**Rust Testing Best Practices:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    
    #[test]
    fn test_momentum_calculation() -> Result<()> {
        // Test with known data
        let prices = vec![100.0, 102.0, 101.0];
        let momentum = calculate_momentum(&prices)?;
        assert!((momentum - 0.01).abs() < f64::EPSILON);
        Ok(())
    }
}
```

### AlgoTrading Project Structure

```
src/
├── main.rs           # Application entrypoint & trading loop
├── config.rs         # Configuration management (config.json)
├── connection.rs     # TWS API connection handling
├── market_data.rs    # Real-time market data processing
├── momentum.rs       # Momentum strategy implementation
├── orders.rs         # Order management system
├── portfolio.rs      # Portfolio tracking and analytics
└── security_types.rs # Security definitions (stocks, futures)
tests/                # Unit and integration tests (TO BE CREATED)
├── momentum_tests.rs
├── orders_tests.rs
└── portfolio_tests.rs
docs/                 # Documentation
└── insights.md       # Trading insights and analysis
config.json          # Trading configuration
Cargo.toml           # Rust dependencies
```

### Key Trading Modules

**connection.rs**: Manages TWS API connection
- Handle reconnections gracefully
- Monitor connection health
- Queue orders during disconnections

**market_data.rs**: Process real-time market data
- Validate incoming prices
- Handle missing/delayed data
- Maintain price history for calculations

**momentum.rs**: Core trading strategy
- Calculate momentum scores
- Generate trading signals
- Rank securities by momentum

**orders.rs**: Order execution
- Validate order parameters
- Track order status
- Handle partial fills and rejections

**portfolio.rs**: Position management
- Track positions and P&L
- Calculate exposure and risk metrics
- Generate rebalancing orders

## Problem-Solving Together

When you're stuck or confused:

1. **Stop** - Don't spiral into complex solutions
2. **Break it down** - Use systematic investigation tools
3. **Think deeply** - For complex problems, engage enhanced reasoning
4. **Step back** - Re-read the requirements
5. **Simplify** - The simple solution is usually correct
6. **Ask** - "I see two approaches: [A] vs [B]. Which do you prefer?"

My insights on better approaches are valued - please ask for them!

## Performance & Security

### **Measure First**:

- No premature optimization
- Benchmark before claiming something is faster
- Use appropriate profiling tools for your language
- Focus on algorithmic improvements over micro-optimizations

### **Security Always**:

- Validate all inputs at boundaries
- Use established crypto libraries (never roll your own)
- Parameterized queries for SQL (never concatenate!)
- Sanitize user input and escape outputs
- Follow OWASP guidelines for your stack

## Communication Protocol

### Progress Updates:

```
- Implemented authentication (all tests passing)
- Added rate limiting
- Found issue with token expiration - investigating
```

### Suggesting Improvements:

"The current approach works, but I notice [observation].
Would you like me to [specific improvement]?"

## AlgoTrading Development Commands

### Build & Run Commands:
```bash
# Development build with all checks
cargo build

# Run with debug logging
RUST_LOG=debug cargo run

# Run tests (when implemented)
cargo test

# Run with release optimizations
cargo build --release
cargo run --release

# Check code quality
cargo fmt -- --check
cargo clippy -- -D warnings
```

### Trading-Specific Debugging:
```bash
# Monitor trading activity
RUST_LOG=info,algotrading::orders=debug cargo run

# Test connection only
RUST_LOG=debug cargo run -- --test-connection

# Dry run (no order submission)
cargo run -- --dry-run
```

## Technical Mastery Progression

### Current Focus: Algorithmic Trading Systems
- Target concept: Building reliable, safe trading systems in Rust
- Learning method: Test-driven development with market scenarios
- Knowledge gap: TWS API edge cases, market microstructure

### Trading-Specific Mastery Areas:
- **Market Mechanics**: Understanding order types, market microstructure
- **Risk Management**: Position sizing, exposure limits, drawdown control
- **Strategy Development**: Backtesting, optimization, overfitting prevention
- **System Reliability**: Connection handling, error recovery, data validation
- **Performance**: Low-latency order submission, efficient data processing

### Rust & Trading Mastery Progression:
- **Async Programming**: Tokio patterns for concurrent market data and orders
- **Error Handling**: Graceful degradation when market conditions change
- **Testing Strategies**: Mocking market conditions and API responses
- **Performance Optimization**: Minimizing latency in critical paths
- **Safety Patterns**: Preventing catastrophic trading errors through types

## Working Together

- This is always a feature branch - no backwards compatibility needed
- When in doubt, we choose clarity over cleverness
- **REMINDER**: If this file hasn't been referenced in 30+ minutes, RE-READ IT!

Avoid complex abstractions or "clever" code. The simple, obvious solution is probably better, and my guidance helps you stay focused on what matters.

## Trading-Specific Best Practices

### Order Safety Checklist:
Before ANY order submission:
- [ ] Validate symbol exists and is tradable
- [ ] Check position size against limits
- [ ] Verify order type is appropriate
- [ ] Confirm using paper trading port
- [ ] Log order details before submission

### Market Data Validation:
- Sanity check prices (no negative values, reasonable ranges)
- Handle stale data (timestamp checks)
- Validate against previous values (spike detection)
- Have fallback behavior for missing data

### Risk Management Implementation:
```rust
// ALWAYS validate position sizes
fn validate_position_size(size: f64, config: &Config) -> Result<f64> {
    if size > config.max_position_size {
        warn!("Position size {} exceeds limit {}", size, config.max_position_size);
        return Ok(config.max_position_size);
    }
    if size <= 0.0 {
        return Err(anyhow!("Invalid position size: {}", size));
    }
    Ok(size)
}
```

### Connection Resilience:
- Implement exponential backoff for reconnections
- Queue orders during disconnections
- Alert on extended disconnections
- Gracefully handle partial data

### Logging Standards:
```rust
// Trading decisions MUST be logged
info!("Momentum signal: {} score={:.4} rank={}", symbol, score, rank);
info!("Submitting order: {} {} {} @ {}", action, quantity, symbol, order_type);
warn!("Risk limit breached: exposure={:.2}% limit={:.2}%", exposure, limit);
error!("Order rejected: {} reason: {}", order_id, reason);
```
