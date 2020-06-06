import os
from flask import Flask, request, render_template, send_from_directory, url_for, flash, redirect
from forms import RegistrationForm, LoginForm

app = Flask(__name__)
app.config.from_object(__name__)
app.config['SECRET_KEY'] = 'e38425d0dcea98645bee3b0f3f3aaf5b'

dungeon_routes = [
    {
        'title': 'mrdmnd rox',
        'creator': 'mrdmnd',
        'date_created': '2020-01-01',
        'content': 'bloohblahlulwkekw'
    }    
]

@app.route('/favicon.ico')
def favicon():
    return send_from_directory(os.path.join(app.root_path, 'static'), 'icons/favicon.ico')


@app.route('/')
@app.route('/home')
def home():
    return render_template('index.html', dungeon_routes = dungeon_routes)


@app.route('/about')
def about():
    return render_template('about.html')

@app.route('/register', methods=["GET", "POST"])
def register():
    form = RegistrationForm()
    if form.validate_on_submit():
        flash(f'Account created for user {form.username.data}!', 'success')
        return redirect(url_for('home'))

    return render_template('register.html', title='Register', form=form)

@app.route('/login', methods=["GET", "POST"])
def login():
    form = LoginForm()
    if form.validate_on_submit():
        if (form.email.data == 'admin@portunuspl.us' and form.password.data == 'password'):
            flash('Logged into account!', 'success')
            return redirect(url_for('home'))
        else:
            flash('Login unsuccessful. Please check Email and Password.', 'danger')
    return render_template('login.html', title='Login', form=form)




if __name__ == '__main__':
    app.run(debug=True)
