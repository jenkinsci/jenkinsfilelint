pipeline {
    agent any
    parameters {
        notAParam(name: 'X')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
