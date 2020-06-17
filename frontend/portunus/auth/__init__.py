from flask import Blueprint
bp = Blueprint('auth', __name__)
from portunus.auth import routes