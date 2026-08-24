#!/usr/bin/env python3
"""
CLOISON STACK-1 Benchmark - Générateur de Dataset Synthétique Sénégalais

Génère un dataset 100% synthétique pour mesurer le fossé de détection PII
entre un détecteur ouest-africain et une baseline Presidio.

Entités supportées:
- PERSON: Noms sénégalais (prénom + patronyme)
- LOC: Toponymes du Sénégal (régions, villes, quartiers)
- CNI: Carte Nationale d'Identité sénégalaise (13 chiffres, commençant par 1)
- MAIL: Adresses email
- TEL: Numéros de téléphone sénégalais (+221)

AUCUNE donnée PII réelle n'est utilisée.
"""

import json
import hashlib
import random
import re
import string
from dataclasses import dataclass, asdict
from typing import List, Dict, Tuple, Optional
from pathlib import Path
from datetime import datetime


# ==============================================================================
# DONNÉES SYNTHÉTIQUES - AUCUNE PII RÉELLE
# ==============================================================================

# Prénoms sénégalais courants (sources: listes publiques académiques)
# ~50 prénoms masculins et ~50 prénoms féminins
PRENOMS_MASCULIN = [
    "Amadou", "Abdou", "Abdoulaye", "Ahmadou", "Alassane", "Alioune", "Alpha",
    "Assane", "Babacar", "Baye", "Cheikh", "Daouda", "Djibril", "El Hadji",
    "Fallou", "Hamadou", "Harouna", "Ibrahima", "Ismaïla", "Lamine", "Mabaye",
    "Mactar", "Mamadou", "Mamadou Lamine", "Mansour", "Mar", "Matar", "Modou",
    "Mouhamadou", "Moussa", "Mustapha", "Omar", "Ousmane", "Pape", "Papa",
    "Saliou", "Sambar", "Serigne", "Seydou", "Sidi", "Souleymane", "Tamsir",
    "Thierno", "Youssouf", "Yaya", "Yoro", "Bounama", "Doudou", "Gora", "Mody",
    "Abdou Karim", "Abdoul", "Adama", "Amadou Bâ", "Cheikh Ahmadou", "Djim"
]

PRENOMS_FEMININ = [
    "Aïda", "Aïssata", "Aminata", "Amy", "Arame", "Astou", "Awa", "Coumba",
    "Diarra", "Dieynaba", "Fatima", "Fatou", "Fatou Bintou", "Fatou Diène",
    "Fima", "Haby", "Hawa", "Khady", "Khoudia", "Kiné", "Maïmouna", "Marème",
    "Mariama", "Marthe", "Maty", "Mbayang", "Ndèye", "Ngoné", "Nogaye",
    "Oulimata", "Rama", "Safiatou", "Seynabou", "Soda", "Souad", "Soukeyna",
    "Synda", "Thioro", "Yacine", "Yaye", "Aminata Dian", "Adama", "Aissata",
    "Adama Diallo", "Absatou", "Bineta", "Coumba Fall", "Diariatou", "Fama"
]

# Patronymes sénégalais courants (sources: registres publics)
PATRONYMES = [
    "Ba", "Bâ", "Barro", "Cisse", "Cissé", "Dabo", "Diallo", "Diagne", "Dian",
    "Diarra", "Diaw", "Dièye", "Diop", "Diouf", "Drame", "Drâme", "Fall",
    "Faye", "Gadio", "Gaye", "Gueye", "Guèye", "Kâ", "Kane", "Kâne", "Ly",
    "Mané", "Mbaye", "Ndao", "Ndiaye", "Ndiongue", "Ndour", "Niane", "Niang",
    "Niass", "Niasse", "Ndieguene", "Ndong", "N'Dour", "Sall", "Samb", "Sambou",
    "Sané", "Sarr", "Seck", "Sèye", "Sow", "Sy", "Sylla", "Tamba", "Thiam",
    "Thiaw", "Traoré", "Wade", "Diémé", "Fofana", "Gassama", "Konaté", "Koné",
    "Sangaré", "Touré", "Diankha", "Dieng", "Khouma", "Mendy", "Coly", "Bodian"
]

# 14 régions officielles du Sénégal
REGIONS = [
    "Dakar", "Diourbel", "Fatick", "Kaffrine", "Kaolack", "Kédougou", "Kolda",
    "Louga", "Matam", "Sédhiou", "Saint-Louis", "Tambacounda", "Thiès", "Ziguinchor"
]

# Villes principales et communes
VILLES = [
    "Dakar", "Pikine", "Guédiawaye", "Rufisque", "Bargny", "Sébi Ponty",
    "Thiès", "Mbour", "Kaolack", "Saint-Louis", "Ziguinchor", "Tambacounda",
    "Kolda", "Kédougou", "Louga", "Matam", "Fatick", "Diourbel", "Kaffrine",
    "Sédhiou", "Touba", "Mbacké", "Bambey", "Dagana", "Podor", "Richard-Toll",
    "Ndiago", "Kayar", "Méouane", "Tivaouane", "Fissel", "Sindia", "Saly",
    "Somone", "Joal-Fadiouth", "Palmarin", "Djifer", "Foundiougne", "Djoudj"
]

# Quartiers de Dakar
QUARTIERS_DAKAR = [
    "Médina", "Plateau", "Fann", "Point E", "Mermoz", "Sicap", "Liberté",
    "Dieuppeul", "Derklé", "Ouakam", "Ngor", "Yoff", "Parcelles Assainies",
    "Grand Yoff", "Patte d'Oie", "Hann Maristes", "Bel Air", "Colobane",
    "Gueule Tapée", "Fass", "Gorée", "SICAP", "Cité Soleil", "Diamaguène",
    "Sam Notaire", "Keur Massar", "Malika", "Thiaroye", "Bargny", "Diamniadio"
]

# Domaines email synthétiques .sn
DOMAINS_SN = [
    "email.sn", "gmail.sn", "yahoo.sn", "orange.sn", "exemple.sn",
    "organisation.sn", "entreprise.sn", "gouv.sn", "admin.sn", "contact.sn",
    "mail.sn", "webmail.sn", "service.sn", "bureau.sn", "dakar.sn"
]

DOMAINS_GENERIC = [
    "gmail.com", "yahoo.fr", "outlook.com", "hotmail.com", "live.com",
    "protonmail.com", "icloud.com", "mail.com"
]

# Préfixes opérateurs téléphoniques sénégalais — attribution CONFIRMÉE
# (plan national de numérotation, soumission ARTP — ITU T02020000B8, posté
# 2023-11-29, et sources 2026) :
#   mobile : 70 Expresso · 7211 CSU/Hayo · 754-756 MVNO PROMOBILE (Sirius
#            Telecoms Afrique) · 757 MVNO ORIGINES SA · 76 FREE Sénégal
#            (ex-Tigo ; rebrandé YAS — Axian — nov. 2024) · 77/78 Sonatel
#            (Orange) · 790 ADIE ;
#   fixe   : 30 Expresso · 32 FREE · 338/339 Sonatel (Dakar/autres) ·
#            3611 CSU/Hayo · 390/391 ADIE.
# NB :
#   - 75 N'EST PAS Free (correction de l'hypothèse DEPLOY-9 « Free 75 ») :
#     c'est la plage des MVNO (Promobile/Origines) et d'Expresso.
#   - 71 n'apparaît PAS au plan ITU 2023 (signalé par le pilote 08/2026) :
#     conservé en couverture (invariant I1 — un mobile non détecté part en
#     clair), attribution opérateur à confirmer ARTP.
#   - 72 (CSU/Hayo, NDC 7211) et 79 (ADIE, NDC 790) : NDC mobiles officiels
#     du plan ITU 2023 — ajoutés à la couverture (DEPLOY-10, recherche ARTP).
PREFIXES_TEL = {
    "Orange (Sonatel)": ["77", "78"],
    "Free/Yas (ex-Tigo)": ["76"],
    "Expresso": ["70"],
    "CSU/Hayo": ["72"],
    "MVNO (75) / Expresso": ["75"],
    "ADIE": ["79"],
    "71 (signalé pilote, hors plan ITU 2023)": ["71"],
}

# Préfixes téléphoniques FIXES sénégalais (N3+) : 33 Sonatel/Orange,
# 30 Expresso, 32 Tigo/Sentel, 36 Hayo/CSU — code de zone 8 (Dakar) ou 9
# (hors Dakar), NSN 8 chiffres (source : Wikipedia "Telephone numbers in
# Senegal"). Un fixe non détecté partirait en clair (invariant I1).
PREFIXES_TEL_FIXE = ["30", "32", "33", "36"]
ZONES_TEL_FIXE = ["8", "9"]

# Templates de documents par niveau de difficulté
TEMPLATES_SIMPLE = [
    "Nom: {PERSON}. Adresse: {LOC}. Téléphone: {TEL}.",
    "Le citoyen {PERSON} réside à {LOC}. Contact: {TEL}.",
    "Dossier administratif pour {PERSON}, né à {LOC}.",
    "Inscription: {PERSON}, {MAIL}, {LOC}.",
    "Contact professionnel: {PERSON}, {TEL}, {MAIL}.",
    "Demande formulée par {PERSON}, habitant {LOC}.",
    "Client: {PERSON}. Livraison à {LOC}. Tél: {TEL}.",
    "Réservation au nom de {PERSON}, email: {MAIL}.",
    "Propriétaire: {PERSON}, domicilié à {LOC}.",
    "Reçu pour {PERSON}, montant validé par {LOC}.",
    "Identité: {PERSON}. CNI: {CNI}. Téléphone: {TEL}.",
    "Enregistrement de {PERSON}, né le 15/03/1990 à {LOC}.",
    "Fiche individuelle: {PERSON}, {MAIL}, {TEL}, {LOC}.",
    "Déclaration de {PERSON}, résidant à {LOC}, CNI {CNI}.",
    "Personne à contacter: {PERSON} ({TEL}).",
    # N3+ : identifiants contextuels sénégalais (passeport, permis, matricule).
    "Numéro de passeport de {PERSON} : {PASSPORT}.",
    "Permis de conduire n° {PERMIS} délivré à {PERSON}.",
    "Matricule fonction publique : {MATRICULE} ({PERSON}).",
    "Numéro IPRES : {MATRICULE} — retraité {PERSON}.",
]

TEMPLATES_CONTEXTUAL = [
    """Le dossier de Monsieur {PERSON}, né le 12 janvier 1985 à {LOC}, a été transmis 
au service compétent. Pour toute information complémentaire, contactez le {TEL} 
ou envoyez un courriel à {MAIL}. L'intéressé réside actuellement dans le quartier de {LOC}.""",

    """Madame {PERSON}, résidant à {LOC}, a déposé une demande de carte d'identité 
le 3 mars 2024. Son numéro de CNI provisoire est {CNI}. Pour le suivi de sa démarche, 
elle peut être jointe au {TEL} ou par email à {MAIL}.""",

    """Conformément à la réglementation en vigueur, nous informons {PERSON}, 
domicilié à {LOC}, que son dossier a été approuvé. Le numéro de référence 
{CNI} a été enregistré dans notre système. Contact: {TEL}.""",

    """L'entreprise confirme l'embauche de {PERSON}, originaire de {LOC}, 
à compter du 1er janvier prochain. Son adresse email professionnelle sera {MAIL} 
et son numéro de poste sera communiqué au {TEL}.""",

    """Lors de la réunion tenue à {LOC}, le représentant {PERSON} a présenté 
le projet à l'assemblée. Les participants peuvent lui faire part de leurs 
observations à l'adresse {MAIL} ou par téléphone au {TEL}.""",

    """Le comité d'organisation, présidé par {PERSON}, se réunira le mois prochain 
à {LOC}. Les membres souhaitant participer sont priés de confirmer leur présence 
au {TEL} ou par courriel à {MAIL}.""",

    """Suite à l'enquête menée dans le quartier de {LOC}, le citoyen {PERSON} 
a été identifié comme témoin clé. Il peut être contacté au {TEL}. 
Son identité a été vérifiée via la CNI {CNI}.""",

    """L'association des résidents de {LOC}, représentée par {PERSON}, 
souhaite attirer l'attention des autorités sur les conditions de vie. 
Pour tout renseignement: {MAIL} ou {TEL}.""",
]

TEMPLATES_ADVERSARIAL = [
    """Rapport annuel 2024 - Commission régionale de {LOC}

La commission, sous la présidence de Monsieur {PERSON}, a mené une enquête approfondie 
sur les conditions de vie dans les quartiers de {LOC}. Les travaux, qui se sont étendus 
sur une période de six mois, ont impliqué de nombreux acteurs locaux.

Mme {PERSON}, coordinatrice du projet, a souligné l'importance de la participation 
citoyenne. Elle peut être contactée à l'adresse {MAIL} ou au {TEL} pour toute 
question relative aux conclusions du rapport.

Le document de référence porte le numéro {CNI} et a été archivé conformément 
aux procédures en vigueur. Le secrétaire {PERSON} a certifié l'authenticité 
des déclarations recueillies dans le quartier de {LOC}.

Pour toute correspondance, écrire à: {PERSON}, Commission de {LOC}, 
ou envoyer un email à {MAIL}. Tél: {TEL}.

Note: Les données personnelles mentionnées dans ce rapport sont protégées 
conformément à la réglementation sur la protection des données personnelles.""",

    """Procès-verbal de la réunion du conseil municipal de {LOC}

Date: 15 juin 2024
Lieu: Mairie de {LOC}
Président: Monsieur {PERSON}
Secrétaire: {PERSON}

Étaient présents: {PERSON} ({TEL}), {PERSON} ({MAIL}), et plusieurs élus 
des quartiers de {LOC}. L'ordre du jour comportait plusieurs points 
relatifs au développement urbain.

Intervention de M. {PERSON}, conseiller: « Je propose d'étudier le dossier 
référencé {CNI} avant de prendre une décision. »

La séance a été levée à 18h30. Le prochain conseil se tiendra à {LOC}. 
Contact pour les questions: {MAIL} ou {TEL}.

Copies transmises à: {PERSON}, {LOC}; Préfecture de {LOC}; Archives.""",

    """Étude comparative sur les pratiques administratives

Cette étude, réalisée par le bureau {PERSON} & Associés, compare les procédures 
administratives entre les régions de {LOC} et {LOC}. L'enquête de terrain 
a été menée par une équipe de cinq personnes.

Coordonnateur: {PERSON} ({MAIL}, {TEL})
Assistante: {PERSON} ({TEL})
Documentaliste: {PERSON} ({MAIL})

Référence du projet: {CNI}

Les conclusions seront présentées lors d'une conférence à {LOC}. 
Pour assister à l'événement, inscrivez-vous via {MAIL} ou appelez le {TEL}.

Rapport rédigé par {PERSON}, validé par {PERSON}, archivé sous {CNI}.""",
]

# Templates non-PII (vrais négatifs)
TEMPLATES_NON_PII = [
    """Météo du jour: temps ensoleillé avec quelques nuages. Températures comprises 
entre 25 et 32 degrés. Vent faible de direction nord-ouest. Humidité: 65%.""",

    """Recette du riz au poisson - Ingrédients: riz, poisson frais, tomates, oignons, 
huile, sel, poivre. Préparation: 30 minutes. Cuisson: 45 minutes. Pour 4 personnes.""",

    """Mode d'emploi de l'appareil: brancher sur secteur, appuyer sur le bouton marche, 
attendre le voyant vert, puis utiliser normalement. Débrancher après usage.""",

    """Description technique: Le moteur électrique possède une puissance de 1500 watts. 
Rotation: 3000 tours/minute. Poids: 12 kg. Dimensions: 40x30x25 cm.""",

    """Instructions de montage: Assembler les pièces A et B, fixer avec les vis fournies, 
ajuster la hauteur, vérifier la stabilité. Outils nécessaires: tournevis, clé.""",

    """Définition: La photosynthèse est le processus par lequel les plantes convertissent 
la lumière solaire en énergie chimique. Elle produit du glucose et de l'oxygène.""",

    """Notice: Pour entretenir le véhicule, vérifier le niveau d'huile tous les 5000 km. 
Contrôler la pression des pneus mensuellement. Remplacer les filtres selon le kilométrage.""",

    """Article scientifique: L'eau couvre 71% de la surface terrestre. 97% est salée, 
2% est congelée, 1% est liquide et douce. Cycle: évaporation, condensation, précipitation.""",

    """Guide de jardinage: Planter les tomates après les dernières gelées. Espacement: 50 cm. 
Arrosage régulier mais modéré. Récolte 60 à 80 jours après la plantation.""",

    """Informations pratiques: La bibliothèque municipale est ouverte du mardi au samedi, 
de 9h à 18h. Entrée gratuite. Prêt de livres: 3 semaines maximum. Sur place: lecture, wifi.""",

    """Cours de mathématiques: Pour calculer l'aire d'un rectangle, multiplier la longueur 
par la largeur. Exemple: longueur = 8 m, largeur = 5 m, aire = 40 m².""",

    """Histoire: L'indépendance du Sénégal a été proclamée le 4 avril 1960. Le pays 
compte 14 régions administratives. La capitale est Dakar. Population: environ 17 millions.""",

    """Vocabulaire: Le wolof est une langue parlée au Sénégal. Phrases courantes: 
'Nanga def?' (Comment vas-tu?), 'Maa ngi fi' (Je suis là), 'Jërëjëf' (Merci).""",

    """Économie: Le secteur primaire comprend l'agriculture, la pêche et l'élevage. 
Le secteur secondaire transforme les matières premières. Le tertiaire offre des services.""",

    """Géographie: Le fleuve Sénégal forme une frontière naturelle. Longueur: 1790 km. 
Il traverse plusieurs pays et se jette dans l'océan Atlantique près de Saint-Louis.""",
]


# ==============================================================================
# ALGORITHME LUHN POUR CNI SÉNÉGALAISE
# ==============================================================================

def luhn_checksum(digits: str) -> int:
    """
    Calcule le chiffre de contrôle selon l'algorithme de Luhn.
    
    L'algorithme de Luhn (aussi appelé mod 10) est une formule de somme de contrôle
    utilisée pour valider les numéros d'identification.
    
    Étapes:
    1. Doubler un chiffre sur deux en partant de la droite (positions paires)
    2. Si le résultat dépasse 9, soustraire 9
    3. Additionner tous les chiffres
    4. Le chiffre de contrôle = (10 - (somme mod 10)) mod 10
    
    Pour une CNI sénégalaise de 13 chiffres commençant par 1:
    - On génère 12 chiffres (premier = 1, puis 11 aléatoires)
    - On calcule le 13ème chiffre comme checksum Luhn
    """
    total = 0
    # Pour 13 chiffres, on veut calculer le checksum du 13ème
    # Donc on double les positions paires en partant de la droite (avant le checksum)
    # digits = corps de 12 chiffres ; le checksum deviendra le 13e chiffre.
    # Luhn standard : doubler un chiffre sur deux en partant de la droite,
    # le checksum (position 0 de la chaîne inversée complète) n'étant pas doublé.
    # Pour un corps de 12 chiffres, les indices PAIRS du corps inversé
    # correspondent aux positions à doubler (i % 2 == 0).
    reverse_digits = digits[::-1]
    
    for i, digit in enumerate(reverse_digits):
        d = int(digit)
        if i % 2 == 0:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    
    checksum = (10 - (total % 10)) % 10
    return checksum


def generate_cni() -> Tuple[str, str]:
    """
    Génère un numéro CNI sénégalais synthétique valide.
    
    Format: 13 chiffres commençant par '1'
    Validation: checksum Luhn sur les 13 chiffres
    
    Retourne: (cni_sans_espaces, cni_avec_espaces)
    Exemple: ('1752345678017', '175 234 5678 01')
    
    Note: L'algorithme Luhn est couramment utilisé pour les identifiants.
    La structure exacte de la CNI sénégalaise étant non documentée publiquement,
    cette implémentation suit les conventions standard de checksum.
    """
    # Premier chiffre: toujours 1
    base = "1"
    
    # Générer 11 chiffres aléatoires
    middle = ''.join([str(random.randint(0, 9)) for _ in range(11)])
    
    # Concaténer (12 chiffres)
    partial = base + middle
    
    # Calculer le checksum Luhn (13ème chiffre)
    checksum = luhn_checksum(partial)
    
    # CNI complète sans espaces
    cni_full = partial + str(checksum)
    
    # Format avec espaces: 1XX XXX XXXX XX
    cni_formatted = f"{cni_full[0:3]} {cni_full[3:6]} {cni_full[6:10]} {cni_full[10:13]}"
    
    return cni_full, cni_formatted


def validate_cni_luhn(cni: str) -> bool:
    """
    Valide un numéro CNI selon l'algorithme Luhn.
    
    Accepte le format avec ou sans espaces.
    Retourne True si le numéro est valide.
    """
    # Supprimer les espaces
    cni_clean = cni.replace(' ', '')
    
    if len(cni_clean) != 13:
        return False
    
    if not cni_clean.startswith('1'):
        return False
    
    if not cni_clean.isdigit():
        return False
    
    # Vérifier le checksum Luhn
    total = 0
    reverse_digits = cni_clean[::-1]
    
    for i, digit in enumerate(reverse_digits):
        d = int(digit)
        if i % 2 == 1:  # Positions paires en partant de la droite
            d *= 2
            if d > 9:
                d -= 9
        total += d
    
    return total % 10 == 0


# ==============================================================================
# GÉNÉRATION D'ENTITÉS
# ==============================================================================

def generate_person() -> str:
    """Génère un nom complet sénégalais (prénom + patronyme)."""
    prenom = random.choice(PRENOMS_MASCULIN + PRENOMS_FEMININ)
    patronyme = random.choice(PATRONYMES)
    return f"{prenom} {patronyme}"


def generate_loc() -> str:
    """Génère un toponyme sénégalais."""
    loc_type = random.choices(
        ['region', 'ville', 'quartier'],
        weights=[0.2, 0.5, 0.3]
    )[0]
    
    if loc_type == 'region':
        return random.choice(REGIONS)
    elif loc_type == 'ville':
        return random.choice(VILLES)
    else:
        return random.choice(QUARTIERS_DAKAR)


def generate_tel() -> str:
    """Génère un numéro de téléphone sénégalais synthétique (mobile ou fixe)."""
    # 25 % de fixes (préfixes 30/32/33/36) : le jeu doit couvrir les deux
    # familles — un fixe non détecté partirait en clair (invariant I1, N3+).
    if random.random() < 0.25:
        prefix = random.choice(PREFIXES_TEL_FIXE)
        zone = random.choice(ZONES_TEL_FIXE)
        suffix = ''.join([str(random.randint(0, 9)) for _ in range(6)])
        format_choice = random.choice(['international', 'local', 'formatted'])
        if format_choice == 'international':
            return f"+221{prefix}{zone}{suffix}"
        elif format_choice == 'local':
            return f"{prefix}{zone}{suffix}"
        else:
            return f"{prefix} {zone}{suffix[0:2]} {suffix[2:4]} {suffix[4:6]}"

    operator = random.choice(list(PREFIXES_TEL.keys()))
    prefix = random.choice(PREFIXES_TEL[operator])
    
    # Générer 7 chiffres restants
    suffix = ''.join([str(random.randint(0, 9)) for _ in range(7)])
    
    # Format variable
    format_choice = random.choice(['international', 'local', 'formatted'])
    
    if format_choice == 'international':
        return f"+221{prefix}{suffix}"
    elif format_choice == 'local':
        return f"{prefix}{suffix}"
    else:
        # Format: XX XXX XX XX
        return f"{prefix} {suffix[0:3]} {suffix[3:5]} {suffix[5:7]}"


def generate_passport() -> str:
    """
    Génère un numéro de passeport sénégalais synthétique (N3+).

    Format CEDEAO/ICAO observé : 1-2 lettres majuscules + 7-8 chiffres
    (ex. A1234567, AB12345678). Toujours NON confirmé par une source
    normative publique (recherche 24/08/2026 : PRADO/HL7/Wikipedia ne
    documentent pas le format du numéro) — la détection produit reste
    contextuelle (« passeport »), conservatrice (charte §11).
    """
    n_letters = random.choice([1, 1, 2])  # 1 lettre plus fréquent
    letters = ''.join([chr(random.randint(ord('A'), ord('Z'))) for _ in range(n_letters)])
    n_digits = 8 if n_letters == 1 else 7
    digits = ''.join([str(random.randint(0, 9)) for _ in range(n_digits)])
    return f"{letters}{digits}"


def generate_permis() -> str:
    """
    Génère un numéro de permis de conduire sénégalais synthétique (N3+).

    Le permis NUMÉRISÉ (format SN 009, carte plastique) est le seul valable
    depuis le 04/01/2024 (circulaire belge Chapitre 36/Sénégal — l'ancien
    format papier rose n'est plus reconnu). Le format exact du numéro reste
    NON confirmé (observé : 7-10 chiffres) — détection contextuelle
    (« permis de conduire »), conservatrice (charte §11).
    """
    n = random.choice([7, 8, 9, 10])
    return ''.join([str(random.randint(0, 9)) for _ in range(n)])


def generate_matricule() -> str:
    """
    Génère un matricule synthétique de fonctionnaire de l'État / assuré IPRES
    (actifs et retraités, N3+).

    Format CONFIRMÉ sur listes officielles (fonctionpublique.gouv.sn — PV
    inspecteurs + listes CAP, 76 échantillons vérifiés) : 6 chiffres + 1
    lettre de contrôle majuscule (alphabet A-Z sans I ni O), variante avec
    slash (« 515808/G ») ou sans (« 734123F »). L'hypothèse initiale
    « 8-11 chiffres » (DEPLOY-9) était fausse — le jeu doit couvrir le
    format réel pour tester la détection (invariant I1).
    """
    digits = ''.join([str(random.randint(0, 9)) for _ in range(6)])
    letter = random.choice("ABCDEFGHJKLMNPQRSTUVWXYZ")  # A-Z sans I ni O
    if random.random() < 0.5:
        return f"{digits}/{letter}"
    return f"{digits}{letter}"


def generate_mail() -> str:
    """Génère une adresse email synthétique."""
    prenom = random.choice(PRENOMS_MASCULIN + PRENOMS_FEMININ).lower()
    prenom = prenom.replace(' ', '.').replace('é', 'e').replace('ï', 'i')
    patronyme = random.choice(PATRONYMES).lower().replace('é', 'e').replace('è', 'e')
    
    # Variation du format
    format_choice = random.choice(['dot', 'underscore', 'concat'])
    if format_choice == 'dot':
        local = f"{prenom}.{patronyme}"
    elif format_choice == 'underscore':
        local = f"{prenom}_{patronyme}"
    else:
        local = f"{prenom}{patronyme}"
    
    # Ajouter un nombre aléatoire parfois
    if random.random() < 0.3:
        local += str(random.randint(1, 99))
    
    domain = random.choice(DOMAINS_SN + DOMAINS_GENERIC)
    return f"{local}@{domain}"


# ==============================================================================
# STRUCTURES DE DONNÉES
# ==============================================================================

@dataclass
class Entity:
    """Représente une entité PII annotée."""
    type: str
    start: int
    end: int
    text: str


@dataclass
class Document:
    """Représente un document annoté."""
    doc_id: str
    text: str
    entities: List[Entity]
    difficulty: str
    seed: int


# ==============================================================================
# GÉNÉRATEUR DE DATASET
# ==============================================================================

class DatasetGenerator:
    """
    Générateur de dataset synthétique sénégalais pour CLOISON STACK-1.
    
    Usage:
        generator = DatasetGenerator(seed=42)
        documents = generator.generate(n_docs=500)
        generator.save(documents, 'dataset.jsonl')
    """
    
    ENTITY_TYPES = ['PERSON', 'LOC', 'CNI', 'MAIL', 'TEL']
    
    def __init__(self, seed: int = 42):
        """
        Initialise le générateur avec une graine aléatoire.
        
        Args:
            seed: Graine pour la reproductibilité (défaut: 42)
        """
        self.seed = seed
        self.doc_counter = 0
        
    def _reset_seed(self):
        """Réinitialise la graine aléatoire."""
        random.seed(self.seed)
        self.doc_counter = 0
    
    def _fill_template(self, template: str) -> Tuple[str, List[Entity]]:
        """
        Remplit un template avec des entités synthétiques.

        Parcourt les placeholders de gauche à droite et maintient un décalage
        cumulé : chaque substitution décale les positions des placeholders
        suivants de (len(valeur) - len(placeholder)). Les positions finales
        sont donc exactes dans le texte résultant, y compris quand un même
        type (ex. {PERSON}) apparaît plusieurs fois.
        """
        entities = []
        result = template
        offset = 0

        placeholder_pattern = r'\{(\w+)\}'
        matches = list(re.finditer(placeholder_pattern, template))

        for match in matches:
            entity_type = match.group(1)

            if entity_type == 'PERSON':
                entity_text = generate_person()
            elif entity_type == 'LOC':
                entity_text = generate_loc()
            elif entity_type == 'CNI':
                entity_text = generate_cni()[random.choice([0, 1])]
            elif entity_type == 'MAIL':
                entity_text = generate_mail()
            elif entity_type == 'TEL':
                entity_text = generate_tel()
            elif entity_type == 'PASSPORT':
                entity_text = generate_passport()
            elif entity_type == 'PERMIS':
                entity_text = generate_permis()
            elif entity_type == 'MATRICULE':
                entity_text = generate_matricule()
            else:
                continue

            # Position réelle dans le texte en cours de construction
            start = match.start() + offset
            end = start + len(entity_text)

            entities.append(Entity(
                type=entity_type,
                start=start,
                end=end,
                text=entity_text
            ))

            # Remplacer en appliquant le décalage cumulé
            ins_at = match.start() + offset
            result = result[:ins_at] + entity_text + result[match.end() + offset:]

            # Mettre à jour le décalage pour les placeholders suivants
            offset += len(entity_text) - (match.end() - match.start())

        return result, entities

    def _generate_simple_doc(self) -> Tuple[str, List[Entity]]:
        """Génère un document de niveau simple."""
        template = random.choice(TEMPLATES_SIMPLE)
        return self._fill_template(template)
    
    def _generate_contextual_doc(self) -> Tuple[str, List[Entity]]:
        """Génère un document de niveau contextual."""
        template = random.choice(TEMPLATES_CONTEXTUAL)
        return self._fill_template(template)
    
    def _generate_adversarial_doc(self) -> Tuple[str, List[Entity]]:
        """Génère un document de niveau adversarial."""
        template = random.choice(TEMPLATES_ADVERSARIAL)
        return self._fill_template(template)
    
    def _generate_non_pii_doc(self) -> Tuple[str, List[Entity]]:
        """Génère un document sans PII (vrai négatif)."""
        template = random.choice(TEMPLATES_NON_PII)
        return template, []
    
    def generate(self, n_docs: int = 500) -> List[Document]:
        """
        Génère un dataset de documents annotés.
        
        Distribution:
        - 40% simple (200 docs)
        - 40% contextual (200 docs)
        - 20% adversarial (100 docs)
        - 20% non-PII (100 docs) inclus dans le total
        
        Args:
            n_docs: Nombre total de documents (défaut: 500)
            
        Returns:
            Liste de documents annotés
        """
        self._reset_seed()
        documents = []
        
        # Calculer la distribution
        n_non_pii = int(n_docs * 0.20)  # 20 % de vrais négatifs
        n_pii = n_docs - n_non_pii
        n_simple = int(n_pii * 0.40)
        n_contextual = int(n_pii * 0.40)
        n_adversarial = n_pii - n_simple - n_contextual
        
        # Ajuster pour atteindre exactement n_docs
        remaining = n_docs - (n_simple + n_contextual + n_adversarial + n_non_pii)
        n_adversarial += remaining
        
        # Générer les documents par niveau
        for i in range(n_simple):
            text, entities = self._generate_simple_doc()
            doc = Document(
                doc_id=f"doc_{self.doc_counter:04d}",
                text=text,
                entities=entities,
                difficulty="simple",
                seed=self.seed
            )
            documents.append(doc)
            self.doc_counter += 1
        
        for i in range(n_contextual):
            text, entities = self._generate_contextual_doc()
            doc = Document(
                doc_id=f"doc_{self.doc_counter:04d}",
                text=text,
                entities=entities,
                difficulty="contextual",
                seed=self.seed
            )
            documents.append(doc)
            self.doc_counter += 1
        
        for i in range(n_adversarial):
            text, entities = self._generate_adversarial_doc()
            doc = Document(
                doc_id=f"doc_{self.doc_counter:04d}",
                text=text,
                entities=entities,
                difficulty="adversarial",
                seed=self.seed
            )
            documents.append(doc)
            self.doc_counter += 1
        
        for i in range(n_non_pii):
            text, entities = self._generate_non_pii_doc()
            doc = Document(
                doc_id=f"doc_{self.doc_counter:04d}",
                text=text,
                entities=entities,
                difficulty="non_pii",
                seed=self.seed
            )
            documents.append(doc)
            self.doc_counter += 1
        
        # Mélanger les documents
        random.shuffle(documents)
        
        return documents
    
    def save(self, documents: List[Document], filepath: str) -> str:
        """
        Sauvegarde le dataset en format JSONLines.
        
        Args:
            documents: Liste de documents annotés
            filepath: Chemin du fichier de sortie
            
        Returns:
            Hash SHA-256 du fichier
        """
        with open(filepath, 'w', encoding='utf-8') as f:
            for doc in documents:
                doc_dict = {
                    'doc_id': doc.doc_id,
                    'text': doc.text,
                    'entities': [
                        {
                            'type': e.type,
                            'start': e.start,
                            'end': e.end,
                            'text': e.text
                        }
                        for e in doc.entities
                    ],
                    'difficulty': doc.difficulty,
                    'seed': doc.seed
                }
                f.write(json.dumps(doc_dict, ensure_ascii=False) + '\n')
        
        # Calculer le hash SHA-256
        with open(filepath, 'rb') as f:
            sha256_hash = hashlib.sha256(f.read()).hexdigest()
        
        return sha256_hash
    
    def get_gazetteers(self) -> Dict[str, List[str]]:
        """
        Retourne les listes de gazetteers pour PERSON et LOC.
        
        Ces listes peuvent être utilisées pour configurer Presidio.
        """
        return {
            'PERSON': PATRONYMES + PRENOMS_MASCULIN + PRENOMS_FEMININ,
            'LOC': REGIONS + VILLES + QUARTIERS_DAKAR
        }


# ==============================================================================
# POINT D'ENTRÉE
# ==============================================================================

if __name__ == "__main__":
    # Test du générateur
    generator = DatasetGenerator(seed=42)
    
    # Générer un petit dataset de test
    docs = generator.generate(n_docs=10)
    
    print(f"Généré {len(docs)} documents")
    print(f"Exemple: {docs[0].doc_id}")
    print(f"Texte: {docs[0].text[:100]}...")
    print(f"Entités: {len(docs[0].entities)}")
    
    # Test CNI
    cni_full, cni_formatted = generate_cni()
    print(f"\nCNI générée: {cni_formatted}")
    print(f"Validité Luhn: {validate_cni_luhn(cni_full)}")
    
    # Hash du dataset
    hash_sha256 = generator.save(docs, '/tmp/test_dataset.jsonl')
    print(f"\nHash SHA-256: {hash_sha256}")
