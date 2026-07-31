cat << 'INNER_EOF' > update_decisions3.py
import re

with open('src/sim/decisions.rs', 'r') as f:
    content = f.read()

# Instead of `company_type.clone()`, return a `&'static str` based on the type
pattern = r'''            let \(min_interval, max_interval\) = eval_interval_range\(&company\.company_type\);
            let jitter = rng\.gen_range\(min_interval\.\.=max_interval\);
            company\.next_eval_tick = current_tick \+ jitter;
            company\.company_type\.clone\(\)
        \};'''

replace = '''            let (min_interval, max_interval) = eval_interval_range(&company.company_type);
            let jitter = rng.gen_range(min_interval..=max_interval);
            company.next_eval_tick = current_tick + jitter;
            match company.company_type.as_str() {
                "central_bank" => "central_bank",
                "commercial_bank" => "commercial_bank",
                "merchant" => "merchant",
                "small_company" => "small_company",
                "corporation" => "corporation",
                "consumer" => "consumer",
                _ => "",
            }
        };'''

content = re.sub(pattern, replace, content)

with open('src/sim/decisions.rs', 'w') as f:
    f.write(content)
INNER_EOF
python3 update_decisions3.py
cargo bench --bench sim_bench -- bench_decisions_phase
