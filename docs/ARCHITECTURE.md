# Architecture Cognitive Modulaire — Jev+ / Jev-R

**Document de conception — v0.1**

**Auteur :** DrapNard
**Date :** 9 octobre 2026
**Statut :** Architecture proposée, non implémentée ni validée expérimentalement
**Orientation :** Recherche en efficacité de l'IA, raisonnement collectif, modularité et apprentissage continu.

---

## 1. Vision du projet

Développer une architecture d'intelligence artificielle distribuée reposant sur une multitude de petits modèles spécialisés qui coopèrent pour résoudre des tâches normalement confiées à des modèles beaucoup plus volumineux.

Le système doit pouvoir comprendre les demandes humaines, décomposer les objectifs, raisonner en plusieurs étapes, utiliser des experts, vérifier ses résultats, mémoriser ses réussites et acquérir de nouvelles compétences.

L'objectif n'est pas de créer un modèle monolithique toujours plus gros, mais d'améliorer le rapport entre capacité de résolution et ressources consommées.

### Objectifs fondamentaux

- **Modularité totale :** chaque modèle et service peut être remplacé indépendamment.
- **Calcul parcimonieux :** seuls les modèles nécessaires sont activés.
- **Raisonnement collectif :** plusieurs modules spécialisés peuvent résoudre ensemble un problème complexe.
- **Apprentissage progressif :** de nouvelles compétences peuvent être acquises sans reconstruction systématique des poids.
- **Infrastructure légère :** le prototype doit fonctionner sur du matériel grand public.
- **Communication efficace :** les modèles échangent des messages binaires structurés, sans répéter inutilement leurs contextes.
- **Sécurité indépendante :** les modèles ne contrôlent pas leurs propres permissions.
- **Recherche reproductible :** tous les gains doivent être comparés à des modèles classiques à budget de calcul équivalent.

**Hypothèse de recherche :** un collectif correctement orchestré de petits modèles pourrait surpasser des modèles plus gros sur certaines tâches, avec une consommation totale inférieure.

Cette hypothèse reste à démontrer.

## 2. Vue générale de l'architecture

Le système comporte sept grands sous-systèmes.

| Sous-système                 | Responsabilité                                                |
| ---------------------------- | ------------------------------------------------------------- |
| Interface linguistique       | Communication entre humain et système                         |
| Jev+ Collective              | Coordination, décisions et allocation du calcul               |
| Jev-R Collective             | Raisonnement, planification et résolution de problèmes        |
| Expert Network               | Exécution des tâches spécialisées                             |
| Cognitive Fabric             | Communication, mémoire et partage des connaissances           |
| Evolution Engine             | Acquisition et amélioration des compétences                   |
| Security & Evaluation Kernel | Permissions, isolation, validation et contrôle du déploiement |

### Organisation logique

**Humain ↔ Interface LLM ↔ Jev+**

Jev+ communique avec Jev-R, le réseau d'experts et les services de mémoire par l'intermédiaire de la Cognitive Fabric.

Jev-R peut consulter les experts, solliciter des vérifications et transmettre de nouvelles stratégies à Jev+.

L'Evolution Engine observe les lacunes identifiées et prépare de nouvelles compétences.

Toutes les opérations ayant des effets externes traversent le Security Kernel.

Le système utilise une **mémoire partagée structurée**, mais pas nécessairement un contexte neuronal partagé : chaque modèle peut posséder ses propres poids, tokenizer et représentations internes.

---

## 3. Interface linguistique — Micro-LLM

### Mission

Servir d'interface entre l'utilisateur et l'architecture cognitive.

Le Micro-LLM n'est pas le responsable du raisonnement global. Il doit principalement comprendre les intentions, identifier les contraintes et produire des réponses compréhensibles.

### Fonctions

- Compréhension du langage naturel et du contexte conversationnel.
- Conversion des demandes en objectifs structurés.
- Détection des ambiguïtés et demande de précisions lorsque nécessaire.
- Traduction des résultats techniques en langage humain.
- Suivi des préférences et des objectifs du projet.
- Résumé des résultats produits par les autres modules.

### Architecture envisagée

Un modèle open-weight de 1 à 3 milliards de paramètres, quantifié, constitue une base d'expérimentation.

Plusieurs profils peuvent utiliser le même backbone : compréhension, reformulation, synthèse et interprétation technique.

À terme, des modèles linguistiques spécialisés pourraient coopérer, mais leur multiplication ne doit jamais être automatique.

**Règle :** un seul modèle partagé doit être privilégié tant que plusieurs modèles distincts n'apportent pas un gain de qualité supérieur à leur surcoût.

---

## 4. Jev+ Collective — Coordination et décisions

### Mission

Assurer la gestion exécutive de l'ensemble du système.

Jev+ doit décider des modules à solliciter, de l'ordre des opérations, des priorités et du budget de calcul.

### Organisation interne

Jev+ peut être composé de plusieurs petits modules décisionnels :

| Module             | Fonction                                            |
| ------------------ | --------------------------------------------------- |
| Router             | Sélectionner les experts                            |
| Scheduler          | Allouer GPU, mémoire et temps                       |
| Evaluator          | Évaluer les résultats disponibles                   |
| Escalator          | Décider si davantage de raisonnement est nécessaire |
| Dependency Manager | Résoudre les dépendances entre tâches               |
| Capability Manager | Identifier les compétences disponibles ou absentes  |

Ces fonctions peuvent être réalisées par un modèle commun avec plusieurs têtes décisionnelles, par des modèles spécialisés, ou parfois par des algorithmes déterministes.

### Propriétés souhaitées

- Décisions probabilistes.
- Évaluation d'un ensemble dynamique de candidats.
- Gestion de l'incertitude.
- Repli lorsqu'aucune option n'est adaptée.
- Routage adaptatif.
- Prise en compte des ressources disponibles.
- Robustesse aux changements d'ordre et de structure des données.

**Attention :** le moteur Jev original ne constitue pas directement cette architecture. Jev+ serait une nouvelle implémentation, éventuellement inspirée de NanoJev.

---

## 5. Jev-R Collective — Réseau de raisonnement

### Mission

Produire des stratégies, résoudre les problèmes complexes et réviser les hypothèses lorsqu'elles échouent.

Contrairement à un Jev-like purement décisionnel, Jev-R doit pouvoir construire progressivement de nouvelles solutions.

### Modules logiques

| Module      | Responsabilité                              |
| ----------- | ------------------------------------------- |
| Planner-R   | Décomposer un objectif en sous-objectifs    |
| Solver-R    | Proposer une solution                       |
| Explorer-R  | Explorer des approches alternatives         |
| Critic-R    | Rechercher les faiblesses et contradictions |
| Verifier-R  | Demander ou interpréter des preuves         |
| Reflector-R | Réviser une stratégie après échec           |

Ces rôles ne nécessitent pas obligatoirement six réseaux neuronaux indépendants.

### Raisonnement récursif

Une tâche complexe peut déclencher plusieurs cycles :

1. Comprendre l'objectif et l'état actuel.
2. Construire plusieurs hypothèses.
3. Sélectionner les stratégies les plus prometteuses.
4. Solliciter les experts ou les outils appropriés.
5. Mesurer les résultats.
6. Réviser les hypothèses incorrectes.
7. Continuer jusqu'à satisfaction des critères ou épuisement du budget.

Un graphe de tâches et une mémoire explicite conservent la progression.

### Architecture neuronale future

Une évolution pourrait intégrer des réseaux récurrents de petite taille, inspirés des Tiny Recursive Models ou des Hierarchical Reasoning Models.

Ce serait un programme de recherche distinct : leurs performances sur des problèmes structurés ne démontrent pas directement une capacité de raisonnement général.

La première version utilisera donc des algorithmes de recherche et des modèles existants, sans entraînement spécifique.

---

## 6. Expert Network — Spécialisation dynamique

### Mission

Effectuer les tâches techniques, documentaires et opérationnelles demandées par Jev+ et Jev-R.

### Experts initiaux envisagés

| Expert        | Domaine                                  |
| ------------- | ---------------------------------------- |
| `rust.s1`     | Rust et écosystème associé               |
| `linux.s1`    | Linux, système et administration         |
| `security.s1` | Analyse défensive et sécurité logicielle |
| `reverse.s1`  | Analyse de binaires et formats           |
| `data.sql.s1` | Bases de données et SQL                  |
| `git.s1`      | Gestion de versions                      |
| `unity.s1`    | Unity et C#                              |
| `trainer.s1`  | Préparation et entraînement des modèles  |

Un expert peut correspondre à un outil, un ensemble documentaire RAG, un adaptateur LoRA ou un modèle neuronal distinct.

### Hiérarchie des compétences

Une compétence générale peut utiliser plusieurs sous-compétences.

Exemple :

`rust.s1 → rust.async.s1 → rust.async.tokio.s1`

Les dépendances sont déclarées dans un registre commun.

### Chargement dynamique

Le système charge seulement les modèles actifs nécessaires et peut désactiver ou décharger ceux qui sont inutilisés.

Les experts partagent autant que possible les mêmes backbones pour limiter l'occupation de la VRAM.

---

## 7. Cognitive Fabric — Communication et mémoire

### Mission

Fournir l'infrastructure commune permettant aux modules de travailler ensemble sans dépendance directe à leurs implémentations.

### Protocole

**FlatBuffers** serait utilisé comme premier format de sérialisation interne.

Le protocole devra prévoir :

- Identifiants de tâche et de message.
- Version du protocole.
- Identité du module émetteur.
- Destination logique.
- Type de message.
- Références vers les données utiles.
- Version du modèle employé.
- Budget et date d'expiration de la tâche.
- Références vers les preuves et les résultats.

Les grandes données seront conservées séparément. Les messages échangeront principalement des identifiants ou des références.

### Communication

Un bus d'événements Rust permettra une communication bidirectionnelle entre modules.

Les messages passeront par un broker qui contrôle les destinataires et les autorisations.

Une séparation sera maintenue entre :

- Les faits observés.
- Les hypothèses.
- Les instructions utilisateur.
- Les données externes non fiables.
- Les résultats vérifiés.
- Les commandes d'exécution.

### Mémoire persistante

La mémoire comportera plusieurs niveaux :

| Mémoire          | Contenu                                 |
| ---------------- | --------------------------------------- |
| Task Memory      | État des tâches en cours                |
| Episodic Memory  | Historique des tentatives et résultats  |
| Semantic Memory  | Documentation et connaissances          |
| Skill Memory     | Compétences et stratégies réutilisables |
| Model Registry   | Versions, capacités et dépendances      |
| Evaluation Store | Benchmarks, résultats et preuves        |

SQLite pourra suffire au début, complété par une recherche documentaire et éventuellement vectorielle.

**FlatBuffers optimise la communication, pas le calcul neuronal lui-même.**

---

## 8. Evolution Engine — Apprentissage et adaptation

### Mission

Permettre au système d'acquérir des compétences tout en conservant celles déjà présentes.

### Quatre niveaux d'apprentissage

**Niveau A — Mémoire immédiate**

Création de nouvelles connaissances, règles de travail et stratégies réutilisables sans modification des poids.

**Niveau B — Skill documentaire**

Recherche de références et création d'une compétence RAG, associée à des outils et des tests.

**Niveau C — Adaptateur neuronal**

Fine-tuning léger par LoRA ou méthode équivalente, à partir de données préparées et validées.

**Niveau D — Reconstruction du modèle**

Nouvelle architecture, nouvel entraînement ou modification profonde des poids.

Le niveau D restera expérimental, car son coût et ses risques sont bien plus élevés.

### Pipeline de création d'un expert

Détection d'une compétence manquante → recherche de datasets → vérification des licences et de la provenance → nettoyage → séparation entraînement/validation/test → adaptation du modèle → tests indépendants → enregistrement du candidat → décision de promotion.

Le collecteur ne devra pas ingérer automatiquement tous les contenus trouvés : les données doivent être filtrées contre l'empoisonnement, les fuites d'informations et les doublons.

### Prévention de l'oubli catastrophique

- Conservation des versions précédentes.
- Backbones gelés lorsque possible.
- Adaptateurs spécialisés indépendants.
- Tests de non-régression.
- Réentraînement global uniquement lorsque justifié.
- Comparaison de toutes les nouvelles versions avec les capacités antérieures.

L'apprentissage pourra produire des candidats pendant le fonctionnement normal du système, mais **la mise à jour du modèle de production restera une opération distincte**.

---

## 9. Calcul adaptatif et gestion des ressources

### Objectif

Minimiser le coût total nécessaire à la résolution vérifiée d'une tâche.

### Modes d'exécution

| Mode     | Fonctionnement                                             |
| -------- | ---------------------------------------------------------- |
| FAST     | Routage minimal, un seul solveur                           |
| STANDARD | Planification, résolution, test                            |
| DEEP     | Exploration de plusieurs stratégies et critique            |
| RESEARCH | Recherche prolongée et expérimentation, sous budget strict |

### Budget cognitif

Chaque tâche reçoit un plafond portant sur :

- Nombre d'appels de modèles.
- Nombre de cycles de raisonnement.
- Tokens d'entrée et de sortie.
- Durée GPU.
- Consommation de mémoire.
- Énergie, si mesurable.
- Nombre d'essais et d'appels d'outils.

Jev+ peut modifier l'allocation à l'intérieur de limites définies, mais ne peut pas lever seul les plafonds imposés.

### Politique de calcul

Un modèle unique constitue l'option par défaut.

Un deuxième modèle ou un critique n'est activé que si le gain attendu justifie son coût.

La récursion s'arrête lorsqu'une solution est vérifiée, que les améliorations deviennent marginales ou que le budget est épuisé.

Les modèles déjà chargés sont réutilisés autant que possible.

---

## 10. Security Kernel — Sécurité indépendante

### Principe

**Les modèles peuvent apprendre de nouvelles capacités, mais ne peuvent pas s'attribuer de nouvelles permissions.**

Le Security Kernel est un ensemble de composants logiciels indépendants de l'apprentissage neuronal.

Il ne doit pas pouvoir être modifié par les modèles autonomes.

### Responsabilités

- Gestion des permissions.
- Isolation des programmes exécutés.
- Contrôle des accès réseau et fichiers.
- Gestion des ressources et quotas.
- Protection des secrets.
- Validation des artefacts.
- Journal d'audit.
- Arrêt d'urgence et rollback.
- Contrôle du cycle de promotion des modèles.

### Menaces à prendre en compte

| Risque                    | Protection prévue                                 |
| ------------------------- | ------------------------------------------------- |
| Prompt injection          | Séparation des instructions et données externes   |
| Data poisoning            | Provenance, quarantaine et validation             |
| Model poisoning           | Tests indépendants et signature des artefacts     |
| Escalade de privilèges    | Permissions minimales appliquées hors des modèles |
| Compromission d'un expert | Isolation des processus et limitation des outils  |
| Boucles incontrôlées      | Budgets d'exécution obligatoires                  |
| Régression du modèle      | Comparaison A/B et rollback                       |
| Manipulation entre agents | Authentification et autorisation des messages     |

Une sandbox réduit les risques, mais n'offre pas de sécurité absolue.

### Promotion des modèles

La production utilise une version stable et identifiable de chaque modèle.

Toute nouvelle version passe par :

1. Construction dans un environnement isolé.
2. Vérification de l'intégrité et de la provenance.
3. Évaluation de la qualité et de la sécurité.
4. Comparaison avec la version actuelle.
5. Autorisation de déploiement selon le niveau de risque.
6. Déploiement limité ou canary.
7. Supervision, puis promotion ou rollback.

Les tâches déjà commencées doivent conserver la version du modèle avec laquelle elles ont été initialisées, sauf migration explicitement prise en charge.

Les capacités sensibles nécessiteront une revue humaine.

---

## 11. Politique open source et publication responsable

### Stratégie

Concevoir le projet comme un framework ouvert, sans supposer que toutes ses capacités futures pourront être publiées sans risque.

### Composants initialement publiables

- Infrastructure de communication.
- Gestion des modèles et plugins.
- Protocoles et interfaces.
- Outils de profiling.
- Benchmarks généraux.
- Outils d'isolation.
- Experts de base évalués.

### Recherche nécessitant une évaluation préalable

- Améliorations majeures de l'efficacité du raisonnement.
- Mécanismes avancés d'auto-amélioration.
- Modules présentant de nouvelles capacités à haut risque.
- Techniques de transfert de performances à grande échelle.

Une publication majeure ne doit pas être déclenchée automatiquement par un résultat de benchmark.

### Limite fondamentale

Un fork peut supprimer les restrictions logicielles d'un projet ouvert.

Il n'existe donc pas de garantie permettant de contrôler les usages de toutes les copies distribuées.

La stratégie doit s'appuyer sur l'évaluation des capacités avant diffusion, et non uniquement sur les protections contenues dans le code.

---

## 12. Implémentation technique initiale

| Composant             | Technologie envisagée                                    |
| --------------------- | -------------------------------------------------------- |
| Orchestrateur         | Rust + Tokio                                             |
| Communication         | FlatBuffers                                              |
| IPC local             | Unix Domain Sockets                                      |
| Interface LLM         | Petit modèle open-weight quantifié                       |
| Décisions Jev+        | Modèle existant ou routeur déterministe                  |
| Raisonnement Jev-R    | Recherche récursive + modèles existants                  |
| Exécution des modèles | llama.cpp ou backend spécialisé                          |
| Fine-tuning ultérieur | PyTorch + PEFT                                           |
| Mémoire               | SQLite et stockage de fichiers                           |
| Artefacts neuronaux   | safetensors                                              |
| Isolation             | Processus isolés, conteneurs ou micro-VM selon le risque |
| Observabilité         | Traces structurées, métriques GPU et journaux            |

### Matériel cible initial

- NVIDIA RTX 3060, 12 Go de VRAM.
- 48 Go de RAM système.

Aucun service cloud payant ne doit être requis pour le prototype initial.

### Politique GPU

- Maintenir uniquement les petits modèles nécessaires en mémoire.
- Réutiliser les poids communs.
- Charger les adaptateurs à la demande.
- Éviter les copies CPU/GPU inutiles.
- Suspendre ou différer l'entraînement en cas de manque de ressources.
- Mesurer systématiquement les coûts des communications et des rechargements.

---

## 13. Benchmarks et validation scientifique

Le projet doit démontrer ses gains face à des modèles standards, sans favoriser artificiellement l'architecture distribuée.

### Références

Comparer :

- Un petit LLM seul.
- Le même LLM avec des outils.
- Le même LLM avec recherche récursive.
- Le collectif Jev+ / Jev-R.
- Le collectif avec mémoire.
- Le collectif avec experts spécialisés.
- Un modèle plus volumineux de référence.

### Indicateurs

| Indicateur                             | Importance |
| -------------------------------------- | ---------- |
| Taux de réussite vérifiée              | Critique   |
| Calcul total par tâche                 | Critique   |
| Joules par tâche résolue               | Critique   |
| Latence                                | Élevée     |
| VRAM maximale                          | Élevée     |
| Robustesse hors distribution           | Critique   |
| Taux de régression après apprentissage | Critique   |
| Nombre d'appels de modèles             | Élevée     |
| Résistance aux entrées malveillantes   | Critique   |

### Benchmark initial

Créer un ensemble reproductible de 100 à 200 tâches techniques et de raisonnement, comprenant des cas nouveaux, des cas ambigus et des problèmes impossibles à résoudre correctement sans outils.

Les mêmes critères et budgets doivent être appliqués aux modèles comparés.

**Premier critère de succès :** améliorer significativement le taux de résolution d'un petit modèle tout en restant compétitif en coût total face à un modèle plus gros.

L'égalité ou la supériorité sur GPT Terra n'est pas considérée comme acquise.

---

## 14. Roadmap de développement

| Version | Fonctionnalité                          | Critère de réussite                                   |
| ------- | --------------------------------------- | ----------------------------------------------------- |
| V0.1    | Cognitive Fabric et registre de modules | Communication typée et remplaçabilité                 |
| V0.2    | Interface LLM + Jev+ initial            | Routage et gestion d'objectifs                        |
| V0.3    | Jev-R récursif                          | Résolution de problèmes avec traces vérifiables       |
| V0.4    | Collaboration multi-rôles               | Gain démontré à budget égal                           |
| V0.5    | Experts dynamiques                      | Chargement et partage des compétences                 |
| V0.6    | Mémoire persistante et RAG              | Réutilisation de solutions validées                   |
| V0.7    | Evolution Engine                        | Création de candidats sans déploiement automatique    |
| V0.8    | LoRA et hot-swap                        | Adaptation validée sans régression significative      |
| V0.9    | Apprentissage collectif                 | Amélioration mesurée des décisions et stratégies      |
| V1.0    | Architecture intégrée                   | Benchmarks, sécurité, documentation, reproductibilité |

### Recherche au-delà de V1.0

- Encodeur structure-agnostic.
- Nouvelles architectures de décision.
- Raisonnement latent récursif.
- Entraînement conjoint de modules.
- Coévolution contrôlée.
- Compression et distillation des expériences.
- Optimisation de l'allocation du calcul.
- Étude de la transférabilité des gains à grande échelle.

Ces travaux pourront être menés séparément du framework de production.

---

## 15. Règles d'architecture non négociables

1. Aucun composant neuronal n'est irremplaçable.
2. Aucun modèle ne possède implicitement les permissions d'un autre.
3. Aucun apprentissage ne modifie directement les poids du modèle actif.
4. Aucun entraînement n'est déclenché lorsqu'une solution moins coûteuse suffit.
5. Aucun modèle supplémentaire n'est activé sans justification de son coût.
6. Aucun score produit par un modèle ne constitue à lui seul une preuve de réussite.
7. Aucune donnée non fiable ne devient automatiquement une instruction système.
8. Aucun changement de schéma ne doit silencieusement modifier le sens des données.
9. Aucun benchmark ne peut être réécrit par le modèle qu'il évalue.
10. Aucune découverte majeure n'est publiée sans évaluation préalable de ses conséquences.

---

## 16. Conclusion

L'architecture vise à remplacer la croissance monolithique des modèles par une organisation cognitive modulaire.

La performance proviendrait de cinq mécanismes complémentaires :

**Spécialisation + routage + raisonnement récursif + mémoire + vérification.**

Le modèle linguistique sert d'interface, Jev+ organise l'activité, Jev-R construit les stratégies et les experts effectuent le travail spécialisé.

L'ensemble peut évoluer sans que chaque amélioration nécessite de reconstruire tous les modèles.

Le défi principal reste scientifique : démontrer que la coopération apporte davantage de capacité que de coût supplémentaire.

La stratégie de développement privilégiera donc des expériences légères, mesurables et reproductibles avant toute modification profonde des architectures neuronales.

**Vision finale : une intelligence distribuée, modulaire, économe en calcul, capable d'apprendre progressivement, mais dont les droits d'action et la diffusion restent contrôlés indépendamment de ses capacités.**

---

**Références techniques de départ :**

- [NanoJev](https://github.com/TianyuCodings/NanoJev)
- [Tiny Recursive Models](https://github.com/SamsungSAILMontreal/TinyRecursiveModels)
- [FlatBuffers](https://flatbuffers.dev/)
- [Hugging Face PEFT](https://huggingface.co/docs/peft/)
- [OWASP Agentic AI Security](https://genai.owasp.org/)
- [NIST AI Risk Management Framework](https://www.nist.gov/itl/ai-risk-management-framework)
