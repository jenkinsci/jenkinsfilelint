pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                "${name}" 'hi'
            }
        }
    }
}
