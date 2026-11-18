pipeline {
    agent any
    parameters {
        string(defaultValue: 'x')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
