# EU AI Act Risk Tiering & ISO/IEC 42001 Mapping
## URLCleanseBot — AI Engine Component

> **Regulation**: EU AI Act (Regulation (EU) 2024/1689)  
> **Standard**: ISO/IEC 42001:2023 (AI Management System)  
> **Version**: 1.0.0  
> **Date**: 2026-11-04  
> **Classification**: **Minimal Risk** (with conditions)  

---

## 1. AI System Classification

### 1.1 Is This an "AI System"? (Art. 3(1))
> **"A machine-based system that is designed to operate with varying levels of autonomy and that may exhibit adaptiveness after deployment, and that, for explicit or implicit objectives, infers, from the input it receives, how to generate outputs such as predictions, content, recommendations, or decisions that can influence physical or virtual environments."**

| Criterion | Assessment | Evidence |
|-----------|------------|----------|
| **Machine-based** | ✅ Yes | External LLM API (compute infrastructure) |
| **Varying autonomy** | ✅ Yes | Model selects which parameters to remove |
| **Adaptiveness after deployment** | ❌ No | **Frozen model** — no fine-tuning, no online learning, no feedback loop |
| **Infers from input** | ✅ Yes | Input: URL → Output: sanitized URL (decision) |
| **Generates outputs influencing environment** | ✅ Yes | Output returned to user; affects link sharing |

**Conclusion**: **Yes, it is an AI System** under the Act.  
**Key nuance**: The *adaptiveness* criterion is **not met** — this is a critical differentiator from "AI systems that learn".

---

### 1.2 Role Classification (Art. 2, Art. 3)

| Role | Applies? | Rationale |
|------|----------|-----------|
| **Provider** (Art. 3(3)) | ❌ No | We do not develop, train, or place the model on the market |
| **Deployer** (Art. 3(4)) | ✅ **Yes** | We integrate the API into our bot under our authority |
| **Importer** (Art. 3(5)) | ❌ No | Not importing from third country into EU |
| **Distributor** (Art. 3(6)) | ❌ No | Not making available on the market |
| **Authorized Representative** | ❌ No | Not applicable |

**Obligations flow from Deployer role** (Art. 26–27 for high-risk; Art. 50 for transparency).

---

### 1.3 General-Purpose AI (GPAI) Considerations (Chapter V)
- **We are NOT a GPAI provider** — we consume GPAI via API
- **GPAI obligations fall on the model provider** (OpenAI / custom)
- **Our responsibility**: Due diligence on provider (Art. 26(1)(h) for high-risk deployers)

---

## 2. Risk Tier Determination

### 2.1 Tier Assessment Matrix

| Tier | Criteria (Art. 5, 6, 50, Annex III) | Applies? | Justification |
|------|--------------------------------------|----------|---------------|
| **Unacceptable** (Art. 5) | Subliminal manipulation, social scoring, real-time biometric ID in public, etc. | ❌ No | No prohibited practices |
| **High Risk** (Art. 6 + Annex III) | Listed in Annex III (e.g., critical infrastructure, employment, education, law enforcement, migration, justice, essential services) | ❌ No | **Not in Annex III** — URL sanitization is not a listed high-risk use case |
| **Limited Risk / Transparency** (Art. 50) | AI interacting with humans; emotion recognition; biometric categorization; deepfake generation | ⚠️ **Partial** | **Art. 50(1)**: AI system interacting with natural persons → **transparency obligation applies** |
| **Minimal Risk** (Art. 95) | All other AI systems | ✅ **Yes** | Default tier; voluntary codes of conduct encouraged |

### 2.2 Detailed High-Risk Exclusion Analysis (Annex III)

| Annex III Category | Relevance to URLCleanseBot AI Engine |
|--------------------|--------------------------------------|
| **1. Biometrics** | No |
| **2. Critical Infrastructure** | No |
| **3. Education / Vocational Training** | No |
| **4. Employment / Worker Management** | No |
| **5. Essential Services (credit, insurance, emergency)** | No |
| **6. Law Enforcement** | No |
| **7. Migration / Asylum / Border** | No |
| **8. Administration of Justice / Democratic Processes** | No |

**No match**. The service is a **consumer utility for privacy enhancement**, not a high-risk domain.

### 2.3 Transparency Obligation (Art. 50) — Applicable
> **Art. 50(1)**: "Providers shall ensure that AI systems intended to interact directly with natural persons are designed and developed in such a way that the natural persons concerned are informed that they are interacting with an AI system, unless this is obvious from the circumstances and the context of use."

**Implementation in URLCleanseBot**:
- ✅ **UI Badge**: "🤖 AI Deep Scan" shown in Telegram response when AI path executes
- ✅ **Settings Screen**: Explains "AI sends URL to external service for enhanced cleaning"
- ✅ **Help Command**: Documents AI feature and data flow
- ✅ **Opt-In**: User must explicitly enable (not on by default)

---

## 3. Requirements by Tier

### 3.1 Minimal Risk (Baseline) — **Applies**
| Requirement | Source | Implementation |
|-------------|--------|----------------|
| **AI Literacy** (Art. 4) | AI Act | Maintainers trained on AI Act basics; documented in onboarding |
| **Voluntary Code of Conduct** (Art. 95) | AI Act | Adopt [AI Pact](https://digital-strategy.ec.europa.eu/en/policies/ai-pact) commitments |
| **Documentation** | Best Practice | This file + `model-card.md` + `dpia.md` + `inventory.yaml` |
| **Incident Reporting** | Best Practice | Security policy includes AI-specific incidents |

### 3.2 Transparency (Art. 50) — **Applies**
| Requirement | Implementation |
|-------------|----------------|
| Inform users they interact with AI | 🤖 badge in response; settings description |
| Accessibility of information | Plain language in bot UI; `/help` command |
| No deception | Clear: "enhanced cleaning via AI" not "magic" |

### 3.3 High Risk (Art. 8–15) — **NOT Applicable** (but mapped for reference)
| Requirement | Would Apply If High Risk | Current Status |
|-------------|-------------------------|----------------|
| Risk Management System (Art. 9) | Continuous lifecycle risk mgmt | ✅ DPIA + model card cover this voluntarily |
| Data Governance (Art. 10) | Training/validation/testing data quality | N/A — no training |
| Technical Documentation (Art. 11) | Detailed system documentation | ✅ `model-card.md` + ARCHITECTURE.md |
| Record Keeping (Art. 12) | Automatic logging of events | ⚠️ Partial — request logs only (no URL content) |
| Transparency to Users (Art. 13) | Instructions for use | ✅ Bot UI + `/help` |
| Human Oversight (Art. 14) | Design for human control | ✅ Opt-in + fallback + kill switch (unset key) |
| Accuracy, Robustness, Cybersecurity (Art. 15) | Testing + monitoring | ✅ Golden set eval + circuit breaker |

### 3.4 GPAI Deployer Obligations (Art. 26, 27) — **NOT Applicable** (not high-risk)
*Would apply only if system were high-risk and uses GPAI.*

---

## 4. ISO/IEC 42001:2023 Controls Mapping

> **Scope**: AI Management System (AIMS) for URLCleanseBot AI Engine  
> **Approach**: Selective implementation — controls relevant to *deployer of external inference API*

### 4.1 Clause 4: Context of the Organization
| Control | Application |
|---------|-------------|
| **4.1 Understanding the organization and its context** | Documented in `model-card.md` (Intended Use) + `dpia.md` (Processing Description) |
| **4.2 Understanding the needs and expectations of interested parties** | Users (privacy), Deployer (compliance), Provider (contract), Regulators (AI Act/GDPR) |
| **4.3 Determining the scope of the AIMS** | **In Scope**: AI Engine inference path. **Out of Scope**: Rule-based pipeline, Telegram bot core, DB |

### 4.2 Clause 5: Leadership
| Control | Application |
|---------|-------------|
| **5.1 Leadership and commitment** | Governance owner assigned (Maintainer); policy in `model-card.md` |
| **5.2 AI Policy** | This document + `model-card.md` + `dpia.md` constitute policy |
| **5.3 Roles, responsibilities, authorities** | Deployer: Maintainer. Provider: External. DPO: Assigned per deployment. |

### 4.3 Clause 6: Planning
| Control | Application |
|---------|-------------|
| **6.1 Actions to address risks and opportunities** | `risk-tier.md` (this file) + `dpia.md` Risk Assessment |
| **6.2 AI objectives and planning** | Objective: Privacy-preserving URL cleaning. KPI: Precision/Recall + Latency + Cost |
| **6.3 Planning of changes** | Model version pinning (`AI_MODEL=gpt-3.5-turbo-0125`); re-eval on change |

### 4.4 Clause 7: Support
| Control | Application |
|---------|-------------|
| **7.1 Resources** | External API (provider); Internal: `ai_engine.rs`, config, CI eval |
| **7.2 Competence** | Maintainers: Rust, LLM prompting, GDPR, AI Act basics |
| **7.3 Awareness** | AI Act / GDPR training documented |
| **7.4 Communication** | User-facing: bot UI. Internal: GitHub Issues + this docs folder |
| **7.5 Documented information** | `model-card.md`, `dpia.md`, `risk-tier.md`, `inventory.yaml` |

### 4.5 Clause 8: Operation
| Control | Application |
|---------|-------------|
| **8.1 Operational planning and control** | `pipeline.rs` orchestrates; AI gated by `ai_enabled` + `api_key` |
| **8.2 AI risk assessment** | `dpia.md` Section 3 + this file Section 2 |
| **8.3 AI risk treatment** | `dpia.md` Section 4 (Technical/Organizational/Contractual) |
| **8.4 AI system impact assessment** | `dpia.md` = DPIA (Art. 35 GDPR) + FRIA (Art. 27 AI Act if high-risk) |
| **8.5 AI system lifecycle** | **No training**. Lifecycle: Config → Deploy → Monitor → Update Provider → Re-eval |

### 4.6 Clause 9: Performance Evaluation
| Control | Application |
|---------|-------------|
| **9.1 Monitoring, measurement, analysis, evaluation** | Metrics in `model-card.md` (Precision, Recall, FPR, Latency, Cost) |
| **9.2 Internal audit** | Annual review of this documentation + eval results |
| **9.3 Management review** | Maintainer reviews quarterly; DPO reviews annually |

### 4.7 Clause 10: Improvement
| Control | Application |
|---------|-------------|
| **10.1 Nonconformity and corrective action** | Incident response (Security Policy) + eval regression detection |
| **10.2 Continual improvement** | Golden set expansion; prompt optimization; provider upgrade cycle |

---

### 4.8 Annex A: Control Objectives & Controls (Selected)

| Control | Title | Implementation Status |
|---------|-------|----------------------|
| **A.6.1** | AI policy | ✅ This doc + model-card |
| **A.6.2** | AI objectives | ✅ `model-card.md` Evaluation |
| **A.7.1** | AI risk assessment process | ✅ `dpia.md` |
| **A.7.2** | AI risk treatment process | ✅ `dpia.md` Section 4 |
| **A.7.3** | AI impact assessment | ✅ `dpia.md` (DPIA) |
| **A.8.1** | AI system documentation | ✅ `model-card.md` + ARCHITECTURE.md |
| **A.8.2** | Data for AI systems | ⚠️ N/A (no training) — documented in model-card |
| **A.8.3** | Data provenance | ⚠️ N/A — provider responsibility |
| **A.9.1** | AI system development | ✅ `ai_engine.rs` + CI/CD + tests |
| **A.9.2** | AI system verification/validation | ✅ Golden set + adversarial + CI |
| **A.9.3** | AI system deployment | ✅ Docker + env config + feature flag |
| **A.9.4** | AI system operation/monitoring | ✅ Metrics (latency, errors, cost) |
| **A.10.1** | AI system lifecycle management | ✅ Version pinning + re-eval process |
| **A.10.2** | AI system change management | ✅ Config-driven (model, endpoint); PR review |

---

## 5. Compliance Checklist

### 5.1 EU AI Act (Deployer, Minimal Risk + Art. 50)
| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1 | AI System identified & documented | ✅ | `inventory.yaml`, this file |
| 2 | Risk tier assessed (Minimal) | ✅ | Section 2 |
| 3 | Not in Unacceptable/High-Risk categories | ✅ | Section 2.2 |
| 4 | **Art. 50 Transparency**: Users informed of AI interaction | ✅ | 🤖 badge, settings, `/help` |
| 5 | **Art. 4 AI Literacy**: Staff trained | 📋 | Onboarding checklist |
| 6 | **Art. 95**: Voluntary code of conduct considered | 📋 | AI Pact evaluation |
| 7 | Provider due diligence (if high-risk) | N/A | Not high-risk |
| 8 | Incident reporting process | 📋 | Security policy |

### 5.2 GDPR (Art. 35 DPIA + Art. 28 DPA)
| # | Requirement | Status | Evidence |
|---|-------------|--------|----------|
| 1 | DPIA completed | ✅ | `dpia.md` |
| 2 | Lawful basis: Consent (AI path) + Legitimate Interest (core) | ✅ | `dpia.md` Section 1.5 |
| 3 | Data minimization (pre-redaction, opt-in) | ✅ | `redact.rs`, `user_config.ai_enabled` |
| 4 | DPA with AI provider | 📋 | Deployer responsibility |
| 5 | Transfer safeguards (SCC/adequacy) | 📋 | Deployer: configure EU endpoint |
| 6 | Data subject rights (access, erasure, objection) | ✅ | `/settings` toggle; DB deletion on request |
| 7 | Records of processing (Art. 30) | ✅ | `dpia.md` Appendix A |

### 5.3 ISO/IEC 42001 (Selective)
| # | Requirement | Status |
|---|-------------|--------|
| 1 | AI Policy documented | ✅ |
| 2 | Risk assessment process | ✅ |
| 3 | Impact assessment (DPIA) | ✅ |
| 4 | System documentation | ✅ |
| 5 | Verification & validation | ✅ |
| 6 | Deployment & monitoring | ✅ |
| 7 | Internal audit plan | 📋 Annual |
| 8 | Management review | 📋 Quarterly |

---

## 6. Decision Log

| Date | Decision | Rationale | Author |
|------|----------|-----------|--------|
| 2026-11-04 | Classified as **Minimal Risk** (Art. 50 transparency only) | Not in Annex III; no adaptiveness; consumer privacy tool | Governance Team |
| 2026-11-04 | Deployer role confirmed | We integrate API under our authority; no model development | Governance Team |
| 2026-11-04 | DPIA mandatory (Art. 35) | Systematic evaluation + data egress to third party | DPO |
| 2026-11-04 | ISO 42001 selective implementation | Proportionate to risk; deployer of external API | Governance Team |

---

## 7. Monitoring & Triggers for Re-classification

| Trigger | Action | Timeline |
|---------|--------|----------|
| **Provider adds fine-tuning on our data** | Re-assess: may become Provider role | Immediate |
| **AI used for new purpose (e.g., content classification)** | Re-run Annex III mapping | Before deploy |
| **Regulatory guidance changes** | Review tier | Within 30 days |
| **User base enters regulated domain (health, finance)** | Re-assess high-risk criteria | Before feature launch |
| **Model shows adaptiveness (feedback loop added)** | Re-assess "AI System" definition | Immediate |

---

## Appendices

### A. Key Definitions (AI Act)
| Term | Definition (Art. 3) |
|------|---------------------|
| **AI System** | Machine-based, autonomy, adaptiveness, inference, influence |
| **Provider** | Develops/places on market/puts into service under own name |
| **Deployer** | Uses under own authority (not personal non-professional) |
| **GPAI Model** | Trained on broad data, generality, performs wide range of tasks |
| **High-Risk AI System** | Art. 6: Safety component of regulated product OR Annex III |

### B. References
- [EU AI Act Text (Official)](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32024R1689)
- [EU AI Act — Annex III (High-Risk List)](https://eur-lex.europa.eu/legal-content/EN/TXT/?uri=CELEX:32024R1689#anx_III)
- [ISO/IEC 42001:2023](https://www.iso.org/standard/81230.html)
- [AI Pact (Voluntary Commitments)](https://digital-strategy.ec.europa.eu/en/policies/ai-pact)
- [EDPB Guidelines on AI Act / GDPR Interplay](https://edpb.europa.eu/)