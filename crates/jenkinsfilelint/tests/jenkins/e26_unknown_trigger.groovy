pipeline {
    agent any
    triggers {
        notATrigger('x')
    }
    stages {
        stage('S') {
            steps { echo 'x' }
        }
    }
}
