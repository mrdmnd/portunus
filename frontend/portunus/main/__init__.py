from flask import Blueprint
bp = Blueprint('main', __name__)
from portunus.main import routes
