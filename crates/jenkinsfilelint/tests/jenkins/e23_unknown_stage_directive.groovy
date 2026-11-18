pipeline {
    agent any
    stages {
        stage('S') {
            foo 'bar'
            steps { echo 'x' }
        }
    }
}
