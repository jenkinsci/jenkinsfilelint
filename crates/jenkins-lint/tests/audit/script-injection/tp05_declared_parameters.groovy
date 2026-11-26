pipeline {
    agent any
    parameters {
        string(name: 'VERSION', defaultValue: '1.0')
        text(name: 'NOTES')
        choice(name: 'TARGET', choices: ['staging', 'production'])
        booleanParam(name: 'DRY_RUN', defaultValue: true)
    }
    stages {
        stage('Deploy') {
            steps {
                sh "deploy --version ${params.VERSION} --target ${params.TARGET} --dry-run ${params.DRY_RUN}"
                sh "echo $NOTES"
            }
        }
    }
}
