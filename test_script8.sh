cat << 'INNER_EOF' > update_decisions3.py
import re

with open('src/sim/decisions.rs', 'r') as f:
    content = f.read()

pattern = r'''            let \(min_interval, max_interval\) = eval_interval_range\(&company\.company_type\);
            let jitter = rng\.gen_range\(min_interval\.\.=max_interval\);
            company\.next_eval_tick = current_tick \+ jitter;
            company\.company_type\.clone\(\)
        \};

        let mut orders_to_post = Vec::new\(\);

        // --- Central Bank AI \(Monetary Policy\) ──────────────────────────────
        if company_type == "central_bank" \{'''

replace = '''            let (min_interval, max_interval) = eval_interval_range(&company.company_type);
            let jitter = rng.gen_range(min_interval..=max_interval);
            company.next_eval_tick = current_tick + jitter;
            // Since we only match on string slices below, we can avoid cloning here by matching first,
            // or we could use an enum dispatch. But to keep it simple and minimize cloning,
            // we will extract the enum-like logic.
            company.company_type.clone()
        };

        let mut orders_to_post = Vec::new();

        // --- Central Bank AI (Monetary Policy) ──────────────────────────────
        if company_type == "central_bank" {'''
# Actually let's just make it return an enum or something if it's hot path, or use a match inside the company mutable borrow... Wait, the mutable borrow ends there.
INNER_EOF
